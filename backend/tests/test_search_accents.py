"""Recherche insensible aux accents (F5, docs/plans/phase-2.1-fix.md) : `person_apercu.recherche`
est normalisée par `kyc_unaccent(lower(...))` en base (migration 0006), les requêtes normalisent la
même façon leur `q`. Un seul fichier plutôt qu'un test par surface : les trois consomment le même
mécanisme, un test qui les regroupe rend visible qu'ils ne peuvent pas diverger (D2.10).
"""

import asyncpg
import pytest
from httpx import AsyncClient

import factories


@pytest.fixture
async def melenchon(db_conn: asyncpg.Connection) -> int:
    person_id = await factories.insert_person(
        db_conn, an_uid="PA1", prenom="Jean-Luc", nom="Mélenchon"
    )
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-luc-melenchon")
    await factories.insert_candidate(
        db_conn, person_id=person_id, statut="declare", source_url="https://example.org/annonce"
    )
    await factories.refresh_person_apercu(db_conn)
    return person_id


@pytest.mark.parametrize("q", ["melenchon", "MELENCHON", "mélenchon"])
async def test_home_search_ignores_accents_and_case(
    client: AsyncClient, melenchon: int, q: str
) -> None:
    response = await client.get("/", params={"q": q})

    assert "Mélenchon" in response.text


@pytest.mark.parametrize("q", ["melenchon", "MELENCHON", "mélenchon"])
async def test_directory_search_ignores_accents_and_case(
    client: AsyncClient, melenchon: int, q: str
) -> None:
    response = await client.get("/personnes", params={"q": q})

    assert "Mélenchon" in response.text


@pytest.mark.parametrize("q", ["melenchon", "MELENCHON", "mélenchon"])
async def test_api_candidats_search_ignores_accents_and_case(
    client: AsyncClient, melenchon: int, q: str
) -> None:
    response = await client.get("/api/v1/candidats", params={"q": q})

    assert response.status_code == 200
    noms = [entry["nom"] for entry in response.json()["data"]]
    assert "Mélenchon" in noms


@pytest.mark.parametrize("q", ["melenchon", "MELENCHON", "mélenchon"])
async def test_api_personnes_search_ignores_accents_and_case(
    client: AsyncClient, melenchon: int, q: str
) -> None:
    response = await client.get("/api/v1/personnes", params={"q": q})

    assert response.status_code == 200
    noms = [entry["nom"] for entry in response.json()["data"]]
    assert "Mélenchon" in noms
