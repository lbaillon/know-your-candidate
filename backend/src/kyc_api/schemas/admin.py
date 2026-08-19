"""Modèles Pydantic du back-office — voir docs/plans/phase-3-categorisation.md, section
« Authentification admin (D3.1) »."""

from pydantic import BaseModel


class AdminUser(BaseModel):
    id: int
    github_id: int
    github_login: str
    display_name: str
    actif: bool
