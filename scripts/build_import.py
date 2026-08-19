#!/usr/bin/env python3
"""Vérifie les réponses d'un modèle et les assemble en un fichier d'import.

Voir scripts/README.md. Ce script **ne remplace pas** la validation de l'application : celle-ci
revérifie tout, c'est elle la garde (D3.15, « le fichier entier ou rien »). Il existe pour que
l'erreur se voie ici, sur le poste, avec le numéro de ligne du lot fautif — plutôt qu'après un
aller-retour de dépôt sur un fichier de plusieurs milliers de lignes.

Format attendu des réponses : **TSV**, six colonnes, sans en-tête —

    uid<TAB>theme<TAB>poids<TAB>position_pour<TAB>confiance<TAB>justification

La tabulation, et pas la virgule, parce qu'une justification contient des virgules et que le
guillemetage est ce qu'un modèle rate en premier. La conversion vers le CSV de l'application, elle,
est faite ici par `csv.writer`, qui guillemète correctement par construction.

Bibliothèque standard uniquement : ce script doit tourner sans environnement virtuel.

Usage :
    python3 scripts/build_import.py --export export.json --reponses reponses/ --out import.csv
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from decimal import Decimal, InvalidOperation
from pathlib import Path

SCHEMA_VERSION = 1
COLONNES = (
    "schema_version",
    "scrutin_uid",
    "titre",
    "theme",
    "poids",
    "position_pour",
    "confiance",
    "justification",
)
JUSTIFICATION_MIN = 10
MAX_ERREURS_AFFICHEES = 50
UN = Decimal("1.000")


class Erreurs:
    """Accumule au lieu de s'arrêter à la première : corriger dix erreurs d'un coup vaut mieux que
    dix allers-retours."""

    def __init__(self) -> None:
        self.messages: list[str] = []

    def ajoute(self, origine: str, message: str) -> None:
        self.messages.append(f"{origine} : {message}")

    def __bool__(self) -> bool:
        return bool(self.messages)


def decimale(brut: str, origine: str, champ: str, erreurs: Erreurs) -> Decimal | None:
    """Refuse plus de trois décimales plutôt que d'arrondir en silence : au-delà, c'est le symptôme
    d'un calcul flottant en amont, et l'application refusera le fichier (D3.13)."""
    try:
        valeur = Decimal(brut)
    except InvalidOperation:
        erreurs.ajoute(origine, f"{champ} : {brut!r} n'est pas un nombre")
        return None
    if -valeur.as_tuple().exponent > 3:
        erreurs.ajoute(origine, f"{champ} : {brut!r} a plus de trois décimales")
        return None
    return valeur.quantize(Decimal("0.001"))


def lit_reponses(chemins: list[Path]) -> list[tuple[str, int, list[str]]]:
    lignes: list[tuple[str, int, list[str]]] = []
    for chemin in chemins:
        for numero, brut in enumerate(chemin.read_text(encoding="utf-8").splitlines(), start=1):
            texte = brut.strip()
            if not texte or texte.startswith("#"):
                continue
            # Un modèle rend parfois son tableau dans une clôture Markdown : on l'ignore plutôt
            # que de la compter comme une ligne fautive.
            if texte.startswith("```"):
                continue
            champs = [c.strip() for c in brut.rstrip("\n").split("\t")]
            if champs and champs[0].lower() in ("uid", "scrutin_uid"):
                continue  # en-tête rendu spontanément par le modèle
            lignes.append((chemin.name, numero, champs))
    return lignes


def main() -> int:
    parseur = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parseur.add_argument("--export", type=Path, required=True, help="l'export JSON d'origine")
    parseur.add_argument(
        "--reponses",
        type=Path,
        nargs="+",
        required=True,
        help="fichiers TSV rendus par le modèle, ou répertoires les contenant",
    )
    parseur.add_argument("--out", type=Path, required=True, help="fichier CSV à écrire")
    args = parseur.parse_args()

    export = json.loads(args.export.read_text(encoding="utf-8"))
    titres = {s["scrutin_uid"]: s["titre"] for s in export.get("scrutins", [])}
    themes = {t["slug"]: t for t in export.get("themes", [])}
    if not titres or not themes:
        raise SystemExit(f"{args.export} : export vide ou sans thèmes")

    chemins: list[Path] = []
    for cible in args.reponses:
        chemins.extend(sorted(cible.glob("*.tsv")) if cible.is_dir() else [cible])
    if not chemins:
        raise SystemExit("aucun fichier de réponse trouvé")

    erreurs = Erreurs()
    retenues: list[dict[str, str]] = []
    poids_par_uid: dict[str, Decimal] = {}
    vus: set[tuple[str, str]] = set()

    for fichier, numero, champs in lit_reponses(chemins):
        origine = f"{fichier}:{numero}"
        if len(champs) != 6:
            erreurs.ajoute(origine, f"{len(champs)} colonne(s) au lieu de 6 (séparateur tabulé ?)")
            continue
        uid, theme, poids_brut, position_brut, confiance_brut, justification = champs

        if uid not in titres:
            erreurs.ajoute(origine, f"scrutin inconnu de l'export : {uid!r}")
            continue
        if theme not in themes:
            erreurs.ajoute(origine, f"thème inconnu : {theme!r} (connus : {', '.join(themes)})")
            continue
        if (uid, theme) in vus:
            erreurs.ajoute(origine, f"couple ({uid}, {theme}) déjà présent plus haut")
            continue
        vus.add((uid, theme))

        poids = decimale(poids_brut, origine, "poids", erreurs)
        confiance = decimale(confiance_brut, origine, "confiance", erreurs)
        if poids is None or confiance is None:
            continue
        if not Decimal(0) < poids <= UN:
            erreurs.ajoute(origine, f"poids hors ]0, 1] : {poids}")
            continue
        if not Decimal(0) <= confiance <= UN:
            erreurs.ajoute(origine, f"confiance hors [0, 1] : {confiance}")
            continue

        a_un_axe = bool(themes[theme].get("pole_negatif"))
        position: Decimal | None = None
        if position_brut:
            if not a_un_axe:
                erreurs.ajoute(
                    origine, f"le thème {theme!r} n'a pas d'axe : position à laisser vide"
                )
                continue
            position = decimale(position_brut, origine, "position_pour", erreurs)
            if position is None:
                continue
            if not -UN <= position <= UN:
                erreurs.ajoute(origine, f"position hors [-1, 1] : {position}")
                continue
        elif a_un_axe:
            erreurs.ajoute(origine, f"position manquante sur le thème {theme!r}, qui a un axe")
            continue

        if len(justification.strip()) < JUSTIFICATION_MIN:
            erreurs.ajoute(
                origine,
                f"justification trop courte ({len(justification.strip())} < {JUSTIFICATION_MIN})",
            )
            continue

        poids_par_uid[uid] = poids_par_uid.get(uid, Decimal(0)) + poids
        retenues.append(
            {
                "schema_version": str(SCHEMA_VERSION),
                "scrutin_uid": uid,
                "titre": titres[uid],
                "theme": theme,
                "poids": f"{poids:.3f}",
                "position_pour": f"{position:.3f}" if position is not None else "",
                "confiance": f"{confiance:.3f}",
                "justification": justification.strip(),
            }
        )

    for uid, somme in sorted(poids_par_uid.items()):
        if somme != UN:
            erreurs.ajoute(uid, f"somme des poids = {somme}, attendu exactement 1.000")

    if erreurs:
        print(f"{len(erreurs.messages)} erreur(s) — aucun fichier écrit :", file=sys.stderr)
        for message in erreurs.messages[:MAX_ERREURS_AFFICHEES]:
            print(f"  {message}", file=sys.stderr)
        reste = len(erreurs.messages) - MAX_ERREURS_AFFICHEES
        if reste > 0:
            print(f"  … et {reste} autre(s)", file=sys.stderr)
        return 1

    with args.out.open("w", encoding="utf-8", newline="") as sortie:
        redacteur = csv.DictWriter(sortie, fieldnames=COLONNES)
        redacteur.writeheader()
        redacteur.writerows(retenues)

    couverts = len(poids_par_uid)
    print(f"{len(retenues)} ligne(s) de catégorisation sur {couverts} scrutin(s) → {args.out}")
    print(f"{len(titres) - couverts} scrutin(s) de l'export restent non catégorisés (non touchés)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
