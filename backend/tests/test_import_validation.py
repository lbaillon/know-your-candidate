"""Tests de la validation d'import — voir docs/plans/phase-3-categorisation.md, section
« ⚠️ À concevoir ici, pas ailleurs ». Un fichier de cas par refus dans
`tests/fixtures/imports/`, parcouru par un test paramétré : chacun doit être refusé (par
`labels_io.read_*` ou par `import_validation.build_plan`), les deux fichiers valides doivent
passer. Un refus au bon moment mais au mauvais motif est un demi-bug — voir aussi
`tests/test_labels_io.py` pour les cas qui ne nécessitent aucune base.
"""

from pathlib import Path

import asyncpg
import pytest

from kyc_api import labels_io
from kyc_api.import_validation import build_plan

FIXTURES_DIR = Path(__file__).parent / "fixtures" / "imports"

# bom_utf8.csv est un cas positif, pas un refus : un BOM de tête est toléré (voir
# kyc_api.labels_io.read_csv), pas un signal d'erreur.
VALID_FIXTURES = {"valide.csv", "valide.json", "bom_utf8.csv"}
REFUSAL_FIXTURES = sorted(p.name for p in FIXTURES_DIR.iterdir() if p.name not in VALID_FIXTURES)


async def _seed(conn: asyncpg.Connection) -> None:
    await conn.execute(
        """
        INSERT INTO theme
            (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang, actif)
        VALUES
            ('social-fiscalite', 'Social', 'd', 'négatif', 'positif', 10, true),
            ('environnement', 'Environnement', 'd', 'négatif', 'positif', 20, true),
            ('autre', 'Autre', 'd', NULL, NULL, 99, true),
            ('theme-inactif', 'Inactif', 'd', 'négatif', 'positif', 30, false)
        """
    )
    source_document_id = await conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', 'SC1', 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """
    )
    await conn.execute(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ('SC1', 1, 17, '2024-03-14', 'assemblee', 'SPO', 'titre', 'DecompteNominatif',
                350, 350, 200, 150, 0, 0, 577, $1)
        """,
        source_document_id,
    )


def _read(filename: str, content: str):
    if filename.endswith(".json"):
        return labels_io.read_json(content)
    return labels_io.read_csv(content)


@pytest.mark.parametrize("filename", REFUSAL_FIXTURES)
async def test_each_refusal_fixture_is_rejected(db_conn: asyncpg.Connection, filename: str):
    await _seed(db_conn)
    content = (FIXTURES_DIR / filename).read_text(encoding="utf-8")

    try:
        parsed = _read(filename, content)
    except labels_io.MalformedImport:
        return  # refusé à la lecture : c'est un refus valide (forme)

    plan = await build_plan(db_conn, parsed)
    assert not plan.is_valid, f"{filename} aurait dû être refusé, ne l'a pas été"
    assert plan.problems, f"{filename} : refusé sans expliquer pourquoi"


async def test_valid_csv_fixture_is_accepted(db_conn: asyncpg.Connection):
    await _seed(db_conn)
    content = (FIXTURES_DIR / "valide.csv").read_text(encoding="utf-8")
    parsed = labels_io.read_csv(content)
    plan = await build_plan(db_conn, parsed)
    assert plan.is_valid, plan.problems
    assert plan.counts["creation"] == 1


async def test_valid_json_fixture_is_accepted(db_conn: asyncpg.Connection):
    await _seed(db_conn)
    content = (FIXTURES_DIR / "valide.json").read_text(encoding="utf-8")
    parsed = labels_io.read_json(content)
    plan = await build_plan(db_conn, parsed)
    assert plan.is_valid, plan.problems
    assert plan.counts["creation"] == 1


async def test_a_leading_bom_is_tolerated_not_rejected(db_conn: asyncpg.Connection):
    await _seed(db_conn)
    content = (FIXTURES_DIR / "bom_utf8.csv").read_text(encoding="utf-8")
    parsed = labels_io.read_csv(content)
    plan = await build_plan(db_conn, parsed)
    assert plan.is_valid, plan.problems
    assert plan.counts["creation"] == 1


async def test_error_report_caps_at_fifty_and_reports_the_remainder(db_conn: asyncpg.Connection):
    await _seed(db_conn)
    header = (
        "schema_version,scrutin_uid,legislature,numero,date,titre,type,sort,pour,contre,"
        "abstentions,participation,positions_groupes,url_an,theme,poids,position_pour,confiance,"
        "justification\n"
    )
    # 60 lignes, toutes avec un thème inconnu : 60 erreurs, dont seules 50 doivent être rapportées.
    rows = "".join(
        f"1,SC1,17,1,2024-03-14,titre,type,sort,200,150,0,0.700,,https://example.org,"
        f"inconnu-{i},1.000,0.500,0.700,justification suffisamment longue\n"
        for i in range(60)
    )
    parsed = labels_io.read_csv(header + rows)
    plan = await build_plan(db_conn, parsed)
    assert not plan.is_valid
    assert len(plan.problems) == 50
    assert plan.problems_total == 60


async def test_a_row_referencing_a_theme_without_axis_and_one_with_axis_can_coexist(
    db_conn: asyncpg.Connection,
):
    """Cas positif qui vérifie qu'« autre » (sans axe, sans poids partagé) fonctionne seul,
    complément du refus testé par la fixture position_sur_theme_sans_axe.csv."""
    await _seed(db_conn)
    content = (
        "schema_version,scrutin_uid,legislature,numero,date,titre,type,sort,pour,contre,"
        "abstentions,participation,positions_groupes,url_an,theme,poids,position_pour,confiance,"
        "justification\n"
        "1,SC1,17,1,2024-03-14,titre,type,sort,200,150,0,0.700,,https://example.org,autre,1.000,"
        ",0.700,justification suffisamment longue\n"
    )
    parsed = labels_io.read_csv(content)
    plan = await build_plan(db_conn, parsed)
    assert plan.is_valid, plan.problems
