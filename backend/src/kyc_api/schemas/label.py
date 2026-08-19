"""Modèles Pydantic de la catégorisation — voir docs/plans/phase-3-categorisation.md."""

from datetime import datetime

from pydantic import BaseModel


class LabelEntry(BaseModel):
    """Une ligne de `scrutin_label`, jointe à son thème et à ses auteur·es."""

    theme_slug: str
    theme_libelle: str
    libelle_pole_negatif: str | None
    libelle_pole_positif: str | None
    poids: float
    position_pour: float | None
    confiance: float
    justification: str
    method: str
    author_display_name: str
    created_at: datetime
    updated_at: datetime
    reviewed_by_display_name: str | None
    reviewed_at: datetime | None


class AxisEstimate(BaseModel):
    """La mesure automatique `group_alignment` (D3.7) — jamais publiée (D3.8), lue uniquement
    pour pré-remplir le formulaire du back-office."""

    position_pour: float
    separation: float
    couverture: float
    axis_version: str


class LabelRevisionEntry(BaseModel):
    avant: list[dict[str, str | None]]
    apres: list[dict[str, str | None]]
    method: str
    author_display_name: str
    motif: str | None
    created_at: datetime
