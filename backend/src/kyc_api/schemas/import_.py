"""Modèles Pydantic du schéma d'échange (export/import) — voir
docs/plans/phase-3-categorisation.md, section « Le schéma d'échange, version 1 ». Ce module ne
touche jamais la base : c'est ce qui rend `labels_io` testable sans elle (CLAUDE.md).

Les nombres (`poids`, `position_pour`, `confiance`, `participation`) sont des **chaînes** à trois
décimales, jamais des flottants (D3.13) : c'est ce qui rend un aller-retour export → réimport
identique au caractère près.
"""

from datetime import date, datetime

from pydantic import BaseModel

SCHEMA_VERSION = 1


class ExportTheme(BaseModel):
    slug: str
    libelle: str
    pole_negatif: str | None = None
    pole_positif: str | None = None


class ExportGroupe(BaseModel):
    abrege: str
    membres: int
    pour: int
    contre: int
    abstentions: int
    position_majoritaire: str | None = None


class ExportCategorisation(BaseModel):
    theme: str
    poids: str
    position_pour: str | None = None
    confiance: str
    justification: str


class ExportGenerateur(BaseModel):
    outil: str | None = None
    modele: str | None = None


class ExportScrutin(BaseModel):
    scrutin_uid: str
    legislature: int
    numero: int
    date: date
    titre: str
    type: str
    sort: str | None = None
    pour: int
    contre: int
    abstentions: int
    non_votants: int
    participation: str | None = None
    groupes: list[ExportGroupe] = []
    url_an: str
    categorisation: list[ExportCategorisation] = []


class ExportFiltre(BaseModel):
    statut: str
    theme: str | None = None
    participation_min: str | None = None


class ExportFile(BaseModel):
    schema_version: int = SCHEMA_VERSION
    generated_at: datetime
    filtre: ExportFiltre
    generateur: ExportGenerateur | None = None
    themes: list[ExportTheme]
    scrutins: list[ExportScrutin]
