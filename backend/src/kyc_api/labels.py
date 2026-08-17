"""Vocabulaire d'affichage — voir docs/plans/phase-2-api-ui.md, section « Vocabulaire d'affichage »,
et methodology.md. Une table unique, réutilisée par les pages et par l'API : les formulations sont
imposées, on les recopie, on ne les reformule pas. Complétée au fil des commits qui en ont besoin
(les positions de vote et les causes de non-vote arrivent avec la fiche personne).
"""

CANDIDATE_STATUT_LABELS = {
    "declare": "candidature déclarée",
    "pressenti": "candidature pressentie",
    "retire": "candidature retirée",
}


def candidate_statut_label(statut: str) -> str:
    return CANDIDATE_STATUT_LABELS.get(statut, statut)


NON_INSCRIT_LABEL = "non-inscrit·e"
