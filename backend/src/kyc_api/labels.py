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

# Une mise au point ne modifie jamais le vote (methodology.md § 3) : la phrase le dit
# explicitement, systématiquement accolée à la déclaration.
MISE_AU_POINT_RESULTAT_INCHANGE = "le résultat du scrutin n'a pas été modifié"


def mise_au_point_label(position_declaree: str) -> str:
    if position_declaree in ("pour", "contre"):
        return f"a déclaré après le scrutin avoir voulu voter {position_declaree}"
    if position_declaree == "abstention":
        return "a déclaré après le scrutin avoir voulu s'abstenir"
    return "a déclaré après le scrutin avoir voulu ne pas prendre part au vote"


def categorisation_position_label(
    *,
    position_pour: float | None,
    theme_libelle: str,
    libelle_pole_negatif: str | None,
    libelle_pole_positif: str | None,
) -> str | None:
    """Une position ne s'affiche jamais en nombre nu (plan phase 3, « Pages publiques ») : le
    libellé du pôle vient toujours en premier, le chiffre entre parenthèses. `None` pour un thème
    sans axe (« autre », D3.5) — rien à afficher, pas une case vide.
    """
    if position_pour is None or libelle_pole_negatif is None or libelle_pole_positif is None:
        return None
    pole = libelle_pole_positif if position_pour >= 0 else libelle_pole_negatif
    return f"plutôt : {pole} (position {position_pour:+.1f} sur l'axe {theme_libelle})"


def categorisation_method_label(
    *,
    method: str,
    author_display_name: str,
    created_at: date,
    import_filename: str | None,
    reviewed_by_display_name: str | None,
    reviewed_at: date | None,
) -> str:
    """La méthode est toujours visible sans cliquer (plan phase 3, « Pages publiques ») : une
    catégorisation importée et non relue le dit explicitement, elle ne se contente pas de citer
    le fichier.
    """
    if method == "manual":
        return f"catégorisé par {author_display_name}, le {_format_date_fr(created_at)}"

    base = f"importé du fichier {import_filename}, le {_format_date_fr(created_at)}"
    if reviewed_by_display_name is None or reviewed_at is None:
        return f"{base} — non relu depuis"
    return f"{base}, relu par {reviewed_by_display_name} le {_format_date_fr(reviewed_at)}"
