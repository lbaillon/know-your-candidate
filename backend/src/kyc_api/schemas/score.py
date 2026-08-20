"""Modèles Pydantic des scores — voir docs/plans/phase-4-partis-scores.md. Partagés par les pages
et par l'API (D2.10).
"""

from datetime import date

from pydantic import BaseModel

from kyc_api.schemas.theme import Theme


class PersonOrientation(BaseModel):
    """Une ligne de la fiche personne, un thème éligible du run courant à la fois (D4.7).
    `score`/`incertitude`/`relues` sont `None` sous le seuil de contributions (D4.2) :
    `contributions` et `abstentions` restent le compte réel dans tous les cas, tirés de
    `score_contribution` qui n'applique jamais ce seuil à l'écriture — « données insuffisantes »
    s'accompagne toujours d'un nombre, jamais d'un silence.
    """

    theme: Theme
    score: float | None
    incertitude: float | None
    relues: int | None
    contributions: int
    abstentions: int

    @property
    def seuil_atteint(self) -> bool:
        return self.score is not None


class ScoreContributionDetail(BaseModel):
    """Une ligne de la page d'explication (D4, « L'explication est le produit ») : le scrutin
    contributeur, la position votée, et le poids qu'elle a eu dans le calcul. `apport` est `None`
    pour une abstention (D4.10) — elle est listée, elle n'entre jamais dans la moyenne.
    """

    scrutin_id: int
    scrutin_an_uid: str
    legislature: int
    numero: int
    titre: str
    date_scrutin: date
    position: str
    apport: float | None
    poids: float


class MandatOrientation(BaseModel):
    """Le score d'un groupe restreint à la période d'un mandat de la personne consultée (D4.12) —
    jamais fusionné avec son score personnel (methodology.md § 6, « le score du parti ne se
    mélange jamais au score personnel »).
    """

    organe_an_uid: str
    organe_libelle: str
    organe_libelle_abrege: str | None
    debut: date
    fin: date | None
    theme: Theme
    score: float
    cohesion: float
    contributions: int


class GroupeOrientation(BaseModel):
    """Le score d'un groupe sur toute son existence (D4.4), pour la page `/groupe/{an_uid}`."""

    theme: Theme
    score: float
    cohesion: float
    contributions: int
    membres: int


class MandatOrientationGroup(BaseModel):
    """Toutes les orientations de groupe d'un même mandat, repliées pour l'affichage — un bloc par
    mandat sur la fiche personne (D4.12).
    """

    organe_an_uid: str
    organe_libelle: str
    organe_libelle_abrege: str | None
    debut: date
    fin: date | None
    orientations: list[MandatOrientation]
