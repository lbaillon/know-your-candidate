"""SQL des candidat·es — accueil (D2.1). Un seul module, aucune requête ailleurs (CLAUDE.md)."""

from kyc_api.db import Queryable
from kyc_api.queries.persons import person_apercu_fields
from kyc_api.schemas.person import CandidateCard, CandidateStatus


async def list_candidates(pool: Queryable, *, q: str | None = None) -> list[CandidateCard]:
    """Ordonnée par nom : un ordre neutre, ni chronologique (la date de déclaration ne dit rien du
    sérieux d'une candidature) ni par popularité (que rien en base ne mesure).
    """
    rows = await pool.fetch(
        """
        SELECT pa.*, c.statut, c.source_url, c.source_date, c.note
        FROM candidate c
        JOIN person_apercu pa ON pa.person_id = c.person_id
        WHERE pa.slug IS NOT NULL
          AND ($1::text IS NULL OR pa.recherche LIKE '%' || kyc_unaccent(lower($1)) || '%')
        ORDER BY pa.nom, pa.prenom, pa.person_id
        """,
        q,
    )
    return [
        CandidateCard(
            **person_apercu_fields(row),
            candidature=CandidateStatus(
                statut=row["statut"],
                source_url=row["source_url"],
                source_date=row["source_date"],
                note=row["note"],
            ),
        )
        for row in rows
    ]
