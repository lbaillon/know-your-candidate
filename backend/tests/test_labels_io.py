"""Tests purs du schéma d'échange — voir docs/plans/phase-3-categorisation.md, section « Le
schéma d'échange, version 1 ». Aucune base ici : `labels_io` ne la touche jamais.
"""

from datetime import UTC, date, datetime

import pytest

from kyc_api import labels_io
from kyc_api.labels_io import MalformedImport
from kyc_api.schemas.import_ import (
    ExportCategorisation,
    ExportFile,
    ExportFiltre,
    ExportGenerateur,
    ExportGroupe,
    ExportScrutin,
    ExportTheme,
)


def _sample_export(*, categorized: bool = True) -> ExportFile:
    return ExportFile(
        generated_at=datetime(2026, 8, 18, 9, 0, tzinfo=UTC),
        filtre=ExportFiltre(statut="non_categorises"),
        themes=[
            ExportTheme(
                slug="social-fiscalite",
                libelle="Social / fiscalité",
                pole_negatif="redistribution",
                pole_positif="maîtrise de la fiscalité",
            )
        ],
        scrutins=[
            ExportScrutin(
                scrutin_uid="VTANR5L17V2653",
                legislature=17,
                numero=2653,
                date=date(2025, 6, 24),
                titre="l'ensemble de la proposition de loi, avec une virgule et un « guillemet »",
                type="scrutin public ordinaire",
                sort="adopté",
                pour=140,
                contre=231,
                abstentions=47,
                non_votants=3,
                participation="0.720",
                groupes=[
                    ExportGroupe(
                        abrege="RN",
                        membres=123,
                        pour=122,
                        contre=0,
                        abstentions=0,
                        position_majoritaire="pour",
                    )
                ],
                url_an="https://www.assemblee-nationale.fr/dyn/17/scrutins/2653",
                categorisation=(
                    [
                        ExportCategorisation(
                            theme="social-fiscalite",
                            poids="1.000",
                            position_pour="-0.400",
                            confiance="0.700",
                            justification="vote sur la fiscalité du patrimoine",
                        )
                    ]
                    if categorized
                    else []
                ),
            )
        ],
    )


def test_write_json_then_read_json_round_trips_the_categorisation():
    export = _sample_export()
    parsed = labels_io.read_json(labels_io.write_json(export))

    assert parsed.schema_version == 1
    assert len(parsed.rows) == 1
    row = parsed.rows[0]
    assert row.scrutin_uid == "VTANR5L17V2653"
    assert row.theme == "social-fiscalite"
    assert row.poids == "1.000"
    assert row.position_pour == "-0.400"
    assert row.confiance == "0.700"
    assert row.justification == "vote sur la fiscalité du patrimoine"


def test_write_csv_then_read_csv_round_trips_the_categorisation():
    export = _sample_export()
    parsed = labels_io.read_csv(labels_io.write_csv(export))

    assert parsed.schema_version == 1
    assert len(parsed.rows) == 1
    row = parsed.rows[0]
    assert row.scrutin_uid == "VTANR5L17V2653"
    assert row.theme == "social-fiscalite"
    assert row.poids == "1.000"
    assert row.position_pour == "-0.400"
    assert row.confiance == "0.700"


def test_an_uncategorized_scrutin_produces_no_json_import_row():
    export = _sample_export(categorized=False)
    parsed = labels_io.read_json(labels_io.write_json(export))
    assert parsed.rows == []


def test_an_uncategorized_scrutin_produces_a_single_context_only_csv_row_and_no_import_row():
    export = _sample_export(categorized=False)
    csv_content = labels_io.write_csv(export)
    lines = [line for line in csv_content.splitlines() if line]
    assert len(lines) == 2  # en-tête + une ligne de contexte

    parsed = labels_io.read_csv(csv_content)
    assert parsed.rows == []


def test_csv_context_columns_are_ignored_at_import():
    export = _sample_export()
    csv_content = labels_io.write_csv(export)
    # Modifier une colonne de contexte (titre) ne doit rien changer à l'import : seules
    # schema_version, scrutin_uid et les cinq colonnes de catégorisation sont lues (plan, D3.6).
    tampered = csv_content.replace("proposition de loi", "TITRE MODIFIÉ")
    parsed = labels_io.read_csv(tampered)
    assert parsed.rows[0].scrutin_uid == "VTANR5L17V2653"


def test_read_json_rejects_malformed_json():
    with pytest.raises(MalformedImport):
        labels_io.read_json("{ceci n'est pas du json")


def test_read_json_rejects_an_unknown_schema_version():
    with pytest.raises(MalformedImport):
        labels_io.read_json('{"schema_version": 99, "scrutins": []}')


def test_read_json_rejects_a_missing_schema_version():
    with pytest.raises(MalformedImport):
        labels_io.read_json('{"scrutins": []}')


def test_read_csv_rejects_a_missing_required_column():
    with pytest.raises(MalformedImport):
        labels_io.read_csv("scrutin_uid,theme\nSC1,social-fiscalite\n")


def test_read_csv_rejects_a_semicolon_separated_file():
    with pytest.raises(MalformedImport):
        labels_io.read_csv(
            "schema_version;scrutin_uid;theme;poids;position_pour;confiance;justification\n"
            "1;SC1;social-fiscalite;1.000;0.500;0.700;justification\n"
        )


def test_read_csv_rejects_an_empty_file():
    with pytest.raises(MalformedImport):
        labels_io.read_csv("")


def test_read_json_rejects_a_document_that_is_not_an_object():
    with pytest.raises(MalformedImport):
        labels_io.read_json("[1, 2, 3]")


def test_generateur_modele_is_preserved_through_json_round_trip():
    export = _sample_export()
    export = export.model_copy(
        update={"generateur": ExportGenerateur(outil="script", modele="test-model")}
    )
    parsed = labels_io.read_json(labels_io.write_json(export))
    assert parsed.generateur is not None
    assert parsed.generateur.modele == "test-model"
