"""Modèles Pydantic des thèmes et de leur axe — voir docs/plans/phase-3-categorisation.md, D3.12
(thème et axe fusionnés)."""

from pydantic import BaseModel


class Theme(BaseModel):
    id: int
    slug: str
    libelle: str
    description: str
    libelle_pole_negatif: str | None
    libelle_pole_positif: str | None
    rang: int

    @property
    def has_axis(self) -> bool:
        return self.libelle_pole_negatif is not None
