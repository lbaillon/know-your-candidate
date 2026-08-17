"""SQL sur `person` / `person_apercu` — accueil et annuaire. Tout le SQL vit ici, aucune requête
ailleurs (CLAUDE.md) ; les gabarits et l'API consomment les mêmes modèles Pydantic (D2.10).
"""

from datetime import timedelta
from typing import Any

from asyncpg import Record

from kyc_api.db import Queryable
from kyc_api.schemas.common import Pagination
from kyc_api.schemas.person import GroupeApercu, PersonApercu

# Page suffisamment grande pour tenir l'annuaire sur peu de pages (~3 100 personnes ingérées),
# assez petite pour rester lisible.
DIRECTORY_PAGE_SIZE = 30


def person_apercu_fields(row: Record) -> dict[str, Any]:
    """Champs communs à `PersonApercu` et `CandidateCard`, extraits d'une ligne de
    `person_apercu` (ou d'une jointure qui la contient en entier via `pa.*`).

    « Dernier groupe connu » et non « actuel » (voir migration 0005) : `dernier_groupe_period` est
    un `daterange` borne supérieure exclusive (`[)`), donc `fin` affichée = borne - 1 jour.
    """
    groupe = None
    if row["dernier_groupe_id"] is not None:
        period = row["dernier_groupe_period"]
        fin = period.upper
        if fin is not None:
            fin = fin - timedelta(days=1)
        groupe = GroupeApercu(
            organe_id=row["dernier_groupe_id"],
            libelle=row["dernier_groupe_libelle"],
            libelle_abrege=row["dernier_groupe_abrege"],
            non_inscrit=row["dernier_groupe_non_inscrit"],
            debut=period.lower,
            fin=fin,
        )
    return {
        "person_id": row["person_id"],
        "slug": row["slug"],
        "civilite": row["civilite"],
        "prenom": row["prenom"],
        "nom": row["nom"],
        "date_deces": row["date_deces"],
        "photo_url": row["photo_url"],
        "commons_file": row["commons_file"],
        "licence": row["licence"],
        "licence_url": row["licence_url"],
        "auteur": row["auteur"],
        "votes_total": row["votes_total"],
        "premier_vote": row["premier_vote"],
        "dernier_vote": row["dernier_vote"],
        "groupe": groupe,
        "legislatures": list(row["legislatures"]),
        "est_candidat": row["est_candidat"],
    }


def person_apercu_from_row(row: Record) -> PersonApercu:
    return PersonApercu(**person_apercu_fields(row))


_DIRECTORY_WHERE = """
    pa.slug IS NOT NULL
    AND (
        $1::text IS NULL
        OR (coalesce(pa.prenom, '') || ' ' || coalesce(pa.nom, '')) ILIKE '%' || $1 || '%'
    )
    AND ($2::smallint IS NULL OR $2 = ANY(pa.legislatures))
"""


async def list_directory(
    pool: Queryable, *, q: str | None, legislature: int | None, page: int
) -> tuple[list[PersonApercu], Pagination]:
    page = max(page, 1)
    offset = (page - 1) * DIRECTORY_PAGE_SIZE

    rows = await pool.fetch(
        f"""
        SELECT pa.*
        FROM person_apercu pa
        WHERE {_DIRECTORY_WHERE}
        ORDER BY pa.nom, pa.prenom, pa.person_id
        LIMIT $3 OFFSET $4
        """,
        q,
        legislature,
        DIRECTORY_PAGE_SIZE,
        offset,
    )
    total = await pool.fetchval(
        f"""
        SELECT count(*)
        FROM person_apercu pa
        WHERE {_DIRECTORY_WHERE}
        """,
        q,
        legislature,
    )
    assert isinstance(total, int)

    persons = [person_apercu_from_row(row) for row in rows]
    return persons, Pagination(page=page, page_size=DIRECTORY_PAGE_SIZE, total=total)
