"""Modèles Pydantic pour un scrutin — partagés par la page et l'API (D2.10)."""

from datetime import date

from pydantic import BaseModel

from kyc_api.schemas.common import Source


class ScrutinGroupe(BaseModel):
    """Une ligne de `scrutin_groupe`. `organe_an_uid`/`libelle` sont `None` pour le groupe
    fantôme `PO0` (voir data-sources.md) : un groupe non identifié par la source, pas une erreur
    d'ingestion à masquer.
    """

    rang: int
    organe_an_uid: str | None
    libelle: str | None
    nombre_membres: int
    position_majoritaire: str | None
    pour: int
    contre: int
    abstentions: int
    non_votants: int


class ScrutinDetail(BaseModel):
    scrutin_id: int
    an_uid: str
    numero: int
    legislature: int
    date_scrutin: date
    chambre: str
    titre: str
    type_libelle: str | None
    sort_code: str | None
    demandeur: str | None
    nombre_votants: int
    suffrages_exprimes: int
    pour: int
    contre: int
    abstentions: int
    non_votants: int
    non_votants_volontaires: int
    effectif: int
    participation: float | None
    source: Source
    groupes: list[ScrutinGroupe]
