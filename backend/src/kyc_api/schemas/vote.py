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
    # Une mise au point ne modifie jamais le vote enregistré (methodology.md § 3) : elle est
    # affichée à côté, jamais fusionnée avec `position`. Rare (0,2 % des votes), donc une liste
    # plutôt qu'un champ unique — la source autorise plusieurs déclarations pour un même vote.
    mise_au_point_positions: list[str] = []
