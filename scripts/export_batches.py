#!/usr/bin/env python3
"""Découpe un export de scrutins non catégorisés en lots prêts à donner à un modèle.

Voir scripts/README.md pour le cycle complet. Trois choix structurent ce script :

1. **On regroupe par texte, pas par scrutin.** Sur le corpus de travail (988 scrutins à
   participation ≥ 50 %), il n'y a que 244 textes distincts : le thème est une propriété du texte,
   pas du vote. Grouper garantit que les scrutins d'un même texte partent dans le même lot, donc
   que le modèle leur donne le même thème sans qu'on ait à l'espérer.
2. **On n'envoie que l'identifiant, la date et le titre.** Le bloc `groupes` de l'export fait
   l'essentiel des octets et ne sert à rien pour choisir un thème. Le sens du vote, lui, ne se
   déduit pas du titre pour un amendement — c'est une limite du corpus, pas du format (voir D3.6).
3. **La liste des thèmes est recopiée depuis l'export dans chaque lot**, jamais dans le prompt :
   une liste dupliquée à la main finit toujours par diverger de `db/seeds/themes.toml`.

Bibliothèque standard uniquement : ce script doit tourner sans environnement virtuel.

Usage :
    python3 scripts/export_batches.py export.json --out lots/
    python3 scripts/export_batches.py export.json --out lots/ --textes-par-lot 15
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# Les titres de l'Assemblée nomment toujours le texte porteur après le type d'objet voté :
# « l'amendement n° 51 de Mme X à l'article unique de la proposition de résolution… ». On coupe au
# premier de ces marqueurs et on retire la mention de lecture, qui change d'un scrutin à l'autre
# pour un même texte.
_TEXTE = re.compile(
    r"(projet de loi.*|proposition de loi.*|proposition de résolution.*|projet de loi organique.*)",
    re.IGNORECASE,
)
_LECTURE = re.compile(
    r"\s*\((première|deuxième|nouvelle|troisième|lecture définitive|texte de la commission)"
    r"[^)]*\)\.?\s*$",
    re.IGNORECASE,
)


def texte_porteur(titre: str) -> str:
    """Clé de regroupement : le texte dont le scrutin discute une partie.

    Repli sur le titre entier quand aucun marqueur n'est trouvé (motions de censure, propositions
    de résolution atypiques) : un texte non reconnu forme son propre groupe, ce qui est le
    comportement sûr — jamais deux textes différents fondus en un.
    """
    trouve = _TEXTE.search(titre)
    brut = trouve.group(1) if trouve else titre
    return _LECTURE.sub("", brut).strip().rstrip(".")


def charge_export(chemin: Path) -> dict:
    donnees = json.loads(chemin.read_text(encoding="utf-8"))
    if donnees.get("schema_version") != 1:
        raise SystemExit(
            f"{chemin} : schema_version {donnees.get('schema_version')!r} inattendue (attendu 1)"
        )
    if not donnees.get("scrutins"):
        raise SystemExit(f"{chemin} : aucun scrutin dans l'export")
    return donnees


def bloc_themes(themes: list[dict]) -> str:
    lignes = ["## Thèmes disponibles", ""]
    for theme in themes:
        if theme.get("pole_negatif"):
            lignes.append(
                f"- `{theme['slug']}` — {theme['libelle']} · "
                f"-1 = {theme['pole_negatif']} · +1 = {theme['pole_positif']}"
            )
        else:
            lignes.append(
                f"- `{theme['slug']}` — {theme['libelle']} · sans axe : laisser la position vide"
            )
    return "\n".join(lignes)


def bloc_scrutins(scrutins: list[dict]) -> str:
    lignes = ["## Scrutins à catégoriser", "", "uid\tdate\ttitre"]
    lignes.extend(f"{s['scrutin_uid']}\t{s['date']}\t{s['titre']}" for s in scrutins)
    return "\n".join(lignes)


def groupe_par_texte(scrutins: list[dict]) -> list[list[dict]]:
    groupes: dict[str, list[dict]] = {}
    for scrutin in scrutins:
        groupes.setdefault(texte_porteur(scrutin["titre"]), []).append(scrutin)
    # Tri par date du premier scrutin : les lots suivent la chronologie, ce qui rend une reprise
    # après interruption lisible (« j'en suis à 2024 »).
    return [groupes[cle] for cle in sorted(groupes, key=lambda c: groupes[c][0]["date"])]


def main() -> int:
    parseur = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parseur.add_argument("export", type=Path, help="export JSON de /admin/export")
    parseur.add_argument("--out", type=Path, required=True, help="répertoire des lots à écrire")
    parseur.add_argument(
        "--textes-par-lot",
        type=int,
        default=25,
        help="nombre de textes par lot (défaut : 25, soit ~100 scrutins)",
    )
    args = parseur.parse_args()

    donnees = charge_export(args.export)
    themes = donnees.get("themes", [])
    if not themes:
        raise SystemExit(f"{args.export} : aucun thème dans l'export, un lot serait inutilisable")

    groupes = groupe_par_texte(donnees["scrutins"])
    args.out.mkdir(parents=True, exist_ok=True)

    index: dict[str, list[str]] = {}
    entete_themes = bloc_themes(themes)
    total_octets = 0

    for numero, debut in enumerate(range(0, len(groupes), args.textes_par_lot), start=1):
        lot = [s for groupe in groupes[debut : debut + args.textes_par_lot] for s in groupe]
        contenu = f"{entete_themes}\n\n{bloc_scrutins(lot)}\n"
        fichier = args.out / f"lot-{numero:03d}.txt"
        fichier.write_text(contenu, encoding="utf-8")
        index[fichier.name] = [s["scrutin_uid"] for s in lot]
        total_octets += len(contenu.encode("utf-8"))

    (args.out / "index.json").write_text(
        json.dumps(index, ensure_ascii=False, indent=2), encoding="utf-8"
    )

    print(f"{len(donnees['scrutins'])} scrutins, {len(groupes)} textes distincts")
    print(f"{len(index)} lot(s) écrits dans {args.out}/")
    # Approximation usuelle (~4 octets par jeton) : un ordre de grandeur pour dimensionner la
    # campagne, pas une facture.
    print(f"~{total_octets // 4000} k jetons d'entrée au total, hors prompt")
    return 0


if __name__ == "__main__":
    sys.exit(main())
