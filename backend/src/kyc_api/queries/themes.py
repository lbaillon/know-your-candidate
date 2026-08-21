"""SQL sur `theme` — voir docs/plans/phase-3-categorisation.md. Un seul module, aucune requête
ailleurs (CLAUDE.md).
"""

from kyc_api.db import Queryable
from kyc_api.schemas.theme import Theme

_FIELDS = (
    "id, slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang, "
    "axe_gauche_droite"
)


async def list_active(pool: Queryable) -> list[Theme]:
    rows = await pool.fetch(f"SELECT {_FIELDS} FROM theme WHERE actif ORDER BY rang")
    return [Theme(**dict(row)) for row in rows]


async def get_by_slug(pool: Queryable, slug: str) -> Theme | None:
    row = await pool.fetchrow(f"SELECT {_FIELDS} FROM theme WHERE slug = $1", slug)
    return Theme(**dict(row)) if row is not None else None
