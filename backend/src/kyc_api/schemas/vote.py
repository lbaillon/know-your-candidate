"""Modèles Pydantic pour un vote — partagés par la fiche personne, la liste des votes et l'API
(D2.10)."""

from datetime import date

from pydantic import BaseModel


class VoteRecent(BaseModel):
    scrutin_id: int
    scrutin_an_uid: str
    legislature: int
    numero: int
    titre: str
    date_scrutin: date
    chambre: str
    position: str
    cause_non_vote: str | None
    par_delegation: bool
