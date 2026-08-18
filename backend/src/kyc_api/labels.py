"""Vocabulaire d'affichage — voir docs/plans/phase-2-api-ui.md, section « Vocabulaire d'affichage »,
et methodology.md. Une table unique, réutilisée par les pages et par l'API : les formulations sont
imposées, on les recopie, on ne les reformule pas. Complétée au fil des commits qui en ont besoin
(les positions de vote et les causes de non-vote arrivent avec la fiche personne).
"""

from datetime import date

from kyc_api.timeline import ASSEMBLEE, GP, PARPOL

# Intitulés exacts des pistes de la frise (D2.7, règle 6), tirés de methodology.md § 2.
PISTE_TITLES = {
    GP: "Groupes parlementaires",
    PARPOL: "Rattachements",
    ASSEMBLEE: "Mandats de député·e",
}

CANDIDATE_STATUT_LABELS = {
    "declare": "candidature déclarée",
    "pressenti": "candidature pressentie",
    "retire": "candidature retirée",
}


def candidate_statut_label(statut: str) -> str:
    return CANDIDATE_STATUT_LABELS.get(statut, statut)


NON_INSCRIT_LABEL = "non-inscrit·e"


def _format_date_fr(d: date) -> str:
    return d.strftime("%d/%m/%Y")


def groupe_label(*, libelle: str, debut: date, fin: date | None, non_inscrit: bool) -> str:
    """Phrase complète d'un segment de la piste *Groupes parlementaires* de la frise (D2.7,
    règle 6 et 7). Un mandat non-inscrit ne s'affiche jamais « a siégé au groupe » — c'est une
    règle de méthodologie, pas une préférence graphique (methodology.md § 2).
    """
    debut_txt = _format_date_fr(debut)
    sujet = NON_INSCRIT_LABEL if non_inscrit else f"a siégé au groupe {libelle}"
    if fin is None:
        return f"{sujet} depuis le {debut_txt}"
    return f"{sujet} du {debut_txt} au {_format_date_fr(fin)}"


def rattachement_label(*, libelle: str, debut: date) -> str:
    """Phrase complète d'un segment de la piste *Rattachements* (mandats `PARPOL`). Un
    rattachement au titre du financement de la vie politique est un acte administratif annuel, pas
    une adhésion : **jamais** « membre de » (methodology.md § 2).
    """
    return (
        f"rattaché·e au parti {libelle} au titre du financement de la vie politique, "
        f"déclaration du {_format_date_fr(debut)}"
    )


def mandat_label(*, legislature: int | None, debut: date, fin: date | None) -> str:
    """Phrase complète d'un segment de la piste *Mandats de député·e* (mandats `ASSEMBLEE`)."""
    debut_txt = _format_date_fr(debut)
    legislature_txt = f" ({legislature}e législature)" if legislature else ""
    if fin is None:
        return f"député·e{legislature_txt} depuis le {debut_txt}"
    return f"député·e{legislature_txt} du {debut_txt} au {_format_date_fr(fin)}"


VOTE_POSITION_LABELS = {
    "pour": "a voté pour",
    "contre": "a voté contre",
    "abstention": "s'est abstenu·e",
}

# Les trois seules causes de non-vote publiées par l'AN (data-sources.md) : toutes
# institutionnelles, jamais un désengagement à interpréter.
NON_VOTE_CAUSE_LABELS = {
    "PSE": "présidait la séance",
    "PAN": "présidait l'Assemblée nationale",
    "MG": "membre du Gouvernement",
}


def vote_position_label(position: str, cause_non_vote: str | None = None) -> str:
    """Une cause de non-vote inconnue s'affiche sans glose (« n'a pas pris part au vote », sans
    plus) plutôt que de faire planter la page — la source peut introduire un code inédit.
    """
    if position == "non_votant":
        cause = NON_VOTE_CAUSE_LABELS.get(cause_non_vote) if cause_non_vote else None
        if cause:
            return f"n'a pas pris part au vote : {cause}"
        return "n'a pas pris part au vote"
    return VOTE_POSITION_LABELS.get(position, position)


CONGRES_LABEL = "Congrès du Parlement — députés et sénateurs réunis"
PAR_DELEGATION_LABEL = "vote émis par délégation"
