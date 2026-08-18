"""Modèles Pydantic pour une personne — partagés par les pages et par l'API (D2.10) : les gabarits
et l'API JSON reçoivent exactement la même donnée, jamais une copie qui pourrait diverger.
"""

from datetime import date

from pydantic import BaseModel

from kyc_api.schemas.common import Source


class GroupeApercu(BaseModel):
    """Le dernier groupe parlementaire connu d'une personne — pas « le groupe actuel » : voir la
    migration 0005, section 5, sur pourquoi `person_apercu` ne dépend jamais de `CURRENT_DATE`.
    """

    organe_id: int
    libelle: str
    libelle_abrege: str | None
    non_inscrit: bool
    debut: date
    fin: date | None


class PersonApercu(BaseModel):
    """Une ligne de la vue matérialisée `person_apercu` (migration 0005)."""

    person_id: int
    slug: str
    civilite: str | None
    prenom: str | None
    nom: str | None
    date_deces: date | None
    photo_url: str | None
    commons_file: str | None
    licence: str | None
    licence_url: str | None
    auteur: str | None
    votes_total: int
    premier_vote: date | None
    dernier_vote: date | None
    groupe: GroupeApercu | None
    legislatures: list[int]
    est_candidat: bool


class CandidateStatus(BaseModel):
    statut: str
    source_url: str
    source_date: date
    note: str | None


class CandidateCard(PersonApercu):
    candidature: CandidateStatus


class PersonDetail(BaseModel):
    """La fiche complète d'une personne (`GET /personne/{slug}`) : identité, sans agrégat de vote
    (la frise et les votes récents sont chargés à part, voir queries/persons.py).
    """

    person_id: int
    slug: str
    civilite: str | None
    prenom: str | None
    nom: str | None
    date_deces: date | None
    an_uid: str | None
    wikidata_qid: str | None
    photo_url: str | None
    commons_file: str | None
    licence: str | None
    licence_url: str | None
    auteur: str | None
    candidature: CandidateStatus | None
    source: Source | None
