"""Modèles Pydantic pour un organe (groupe parlementaire) — voir
docs/plans/phase-4-partis-scores.md, D4.8 : seuls les groupes parlementaires sont scorés, tels que
l'Assemblée les publie, jamais un parti ni une continuité entre groupes.
"""

from pydantic import BaseModel


class OrganeDetail(BaseModel):
    organe_id: int
    an_uid: str
    libelle: str
    libelle_abrege: str | None
    legislature: int | None
    is_non_inscrit: bool
