"""Le schéma d'échange (export/import), version 1 — voir docs/plans/phase-3-categorisation.md,
section « Le schéma d'échange, version 1 ». Fonctions pures : ce module transforme des lignes en
objets Pydantic et réciproquement, il ne touche jamais la base. C'est ce qui rend l'aller-retour
testable sans base (`tests/test_labels_io.py`) et le reste (validation, application) testable
sans fichier.

Colonnes CSV, dans cet ordre exact — voir la constante `CSV_FIELDNAMES` :
`schema_version,scrutin_uid,legislature,numero,date,titre,type,sort,pour,contre,abstentions,
participation,positions_groupes,url_an,theme,poids,position_pour,confiance,justification`.
Seules `schema_version`, `scrutin_uid` et les cinq colonnes de catégorisation sont lues à
l'import ; les autres ne sont là que pour le contexte (D3.6) et sont ignorées en lecture.
"""

import csv
import io
import json

from pydantic import BaseModel, ValidationError

from kyc_api.schemas.import_ import SCHEMA_VERSION, ExportFile, ExportGenerateur

CSV_FIELDNAMES = [
    "schema_version",
    "scrutin_uid",
    "legislature",
    "numero",
    "date",
    "titre",
    "type",
    "sort",
    "pour",
    "contre",
    "abstentions",
    "participation",
    "positions_groupes",
    "url_an",
    "theme",
    "poids",
    "position_pour",
    "confiance",
    "justification",
]
_CATEGORISATION_FIELDS = ("theme", "poids", "position_pour", "confiance", "justification")


class MalformedImport(Exception):
    """Un fichier qui ne peut même pas être compris comme un export version 1 — schéma inconnu,
    JSON invalide, colonne obligatoire manquante, séparateur `;` détecté. Pas une erreur de
    contenu (ça, c'est le rôle de la validation d'import, une couche au-dessus) : une erreur de
    forme, refusée avant toute lecture ligne à ligne.
    """


class ImportRow(BaseModel):
    """Une ligne de catégorisation à importer, telle que lue — valeurs non validées, c'est le
    rôle de la couche de validation (commit suivant) de les vérifier contre la base."""

    scrutin_uid: str
    theme: str
    poids: str
    position_pour: str | None
    confiance: str
    justification: str
    source: str  # p. ex. "ligne 12" (CSV) ou "scrutins[3].categorisation[0]" (JSON)


class ParsedImport(BaseModel):
    schema_version: int
    generateur: ExportGenerateur | None = None
    rows: list[ImportRow]


# --- Écriture -----------------------------------------------------------------------------------


def write_json(export: ExportFile) -> str:
    return export.model_dump_json(indent=2)


def write_csv(export: ExportFile) -> str:
    buffer = io.StringIO()
    writer = csv.DictWriter(buffer, fieldnames=CSV_FIELDNAMES, lineterminator="\n")
    writer.writeheader()

    for scrutin in export.scrutins:
        positions_groupes = ";".join(
            f"{g.abrege}:{g.position_majoritaire or 'égalité'}" for g in scrutin.groupes
        )
        base_row = {
            "schema_version": export.schema_version,
            "scrutin_uid": scrutin.scrutin_uid,
            "legislature": scrutin.legislature,
            "numero": scrutin.numero,
            "date": scrutin.date.isoformat(),
            "titre": scrutin.titre,
            "type": scrutin.type,
            "sort": scrutin.sort or "",
            "pour": scrutin.pour,
            "contre": scrutin.contre,
            "abstentions": scrutin.abstentions,
            "participation": scrutin.participation or "",
            "positions_groupes": positions_groupes,
            "url_an": scrutin.url_an,
        }
        if not scrutin.categorisation:
            writer.writerow(
                {
                    **base_row,
                    "theme": "",
                    "poids": "",
                    "position_pour": "",
                    "confiance": "",
                    "justification": "",
                }
            )
        else:
            for categorisation in scrutin.categorisation:
                writer.writerow(
                    {
                        **base_row,
                        "theme": categorisation.theme,
                        "poids": categorisation.poids,
                        "position_pour": categorisation.position_pour or "",
                        "confiance": categorisation.confiance,
                        "justification": categorisation.justification,
                    }
                )

    return buffer.getvalue()


# --- Lecture --------------------------------------------------------------------------------------


def read_json(content: str) -> ParsedImport:
    try:
        data = json.loads(content)
    except json.JSONDecodeError as err:
        raise MalformedImport(f"JSON invalide : {err}") from err

    if not isinstance(data, dict):
        raise MalformedImport("le document JSON doit être un objet")

    _check_schema_version(data.get("schema_version"))

    try:
        export = ExportFile.model_validate(data)
    except ValidationError as err:
        raise MalformedImport(f"structure invalide : {err}") from err

    rows: list[ImportRow] = []
    for i, scrutin in enumerate(export.scrutins):
        for j, categorisation in enumerate(scrutin.categorisation):
            rows.append(
                ImportRow(
                    scrutin_uid=scrutin.scrutin_uid,
                    theme=categorisation.theme,
                    poids=categorisation.poids,
                    position_pour=categorisation.position_pour,
                    confiance=categorisation.confiance,
                    justification=categorisation.justification,
                    source=f"scrutins[{i}].categorisation[{j}]",
                )
            )

    return ParsedImport(
        schema_version=export.schema_version, generateur=export.generateur, rows=rows
    )


def read_csv(content: str) -> ParsedImport:
    first_line = content.splitlines()[0] if content.splitlines() else ""
    if first_line.count(";") > first_line.count(","):
        # Repli explicite (plan, tableau de validation) : un export de tableur français utilise
        # `;` par défaut, et l'erreur générique « colonne manquante » qu'on obtiendrait sinon ne
        # dit rien de compréhensible à qui ne s'y attend pas.
        raise MalformedImport(
            "séparateur « ; » détecté sur la première ligne : ce fichier attend une virgule "
            "comme séparateur de colonnes (export de tableur français ?)"
        )

    reader = csv.DictReader(io.StringIO(content))
    if reader.fieldnames is None:
        raise MalformedImport("fichier CSV vide")

    missing = [
        f
        for f in ("schema_version", "scrutin_uid", *_CATEGORISATION_FIELDS)
        if f not in reader.fieldnames
    ]
    if missing:
        raise MalformedImport(f"colonne(s) obligatoire(s) manquante(s) : {', '.join(missing)}")

    rows: list[ImportRow] = []
    schema_version: int | None = None

    for line_number, record in enumerate(reader, start=2):
        raw_version = (record.get("schema_version") or "").strip()
        try:
            row_version = int(raw_version)
        except ValueError as err:
            raise MalformedImport(
                f"ligne {line_number} : schema_version invalide ({raw_version!r})"
            ) from err
        if schema_version is None:
            _check_schema_version(row_version)
            schema_version = row_version
        elif row_version != schema_version:
            raise MalformedImport(
                f"ligne {line_number} : schema_version {row_version} incohérente avec "
                f"{schema_version} lue plus haut dans le même fichier"
            )

        scrutin_uid = (record.get("scrutin_uid") or "").strip()
        if not scrutin_uid:
            raise MalformedImport(f"ligne {line_number} : scrutin_uid manquant")

        theme = (record.get("theme") or "").strip()
        if not theme:
            # Scrutin présent dans l'export mais non catégorisé (D3.6) : rien à importer pour
            # cette ligne, ce n'est pas une erreur.
            continue

        rows.append(
            ImportRow(
                scrutin_uid=scrutin_uid,
                theme=theme,
                poids=(record.get("poids") or "").strip(),
                position_pour=(record.get("position_pour") or "").strip() or None,
                confiance=(record.get("confiance") or "").strip(),
                justification=record.get("justification") or "",
                source=f"ligne {line_number}",
            )
        )

    if schema_version is None:
        raise MalformedImport("fichier CSV sans aucune ligne de données")

    return ParsedImport(schema_version=schema_version, generateur=None, rows=rows)


def _check_schema_version(value: object) -> None:
    if value is None:
        raise MalformedImport("schema_version absente")
    if value != SCHEMA_VERSION:
        raise MalformedImport(f"schema_version {value!r} inconnue (acceptée : {SCHEMA_VERSION})")
