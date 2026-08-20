"""SQL sur `organe` — voir docs/plans/phase-4-partis-scores.md. Un seul module, aucune requête
ailleurs (CLAUDE.md).
"""

from kyc_api.db import Queryable
from kyc_api.schemas.organe import OrganeDetail


async def get_group_by_an_uid(pool: Queryable, an_uid: str) -> OrganeDetail | None:
    """`code_type = 'GP'` uniquement : un `PARPOL` ou une `ASSEMBLEE` partagent le même espace de
    noms d'`an_uid` mais ne sont pas des groupes parlementaires, et `/groupe/{an_uid}` n'a de sens
    que pour ceux-là (D4.8).
    """
    row = await pool.fetchrow(
        """
        SELECT id AS organe_id, an_uid, libelle, libelle_abrege, legislature, is_non_inscrit
        FROM organe
        WHERE an_uid = $1 AND code_type = 'GP'
        """,
        an_uid,
    )
    return OrganeDetail(**dict(row)) if row is not None else None
