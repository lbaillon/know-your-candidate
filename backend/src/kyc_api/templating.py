from pathlib import Path
from typing import Any, cast

from fastapi.templating import Jinja2Templates

from kyc_api.labels import (
    CONGRES_LABEL,
    MISE_AU_POINT_RESULTAT_INCHANGE,
    NO_ORIENTATIONS_NEVER_SAT_LABEL,
    NO_ORIENTATIONS_UNCATEGORIZED_LABEL,
    NON_INSCRIT_LABEL,
    PAR_DELEGATION_LABEL,
    PISTE_TITLES,
    candidate_statut_label,
    categorisation_method_label,
    categorisation_position_label,
    cohesion_label,
    ecartes_desaccord_label,
    mise_au_point_label,
    orientation_insuffisant_label,
    orientation_label,
    orientation_preuve_label,
    themes_masques_label,
    vote_position_label,
)
from kyc_api.photos import thumbnail_url

templates = Jinja2Templates(directory=Path(__file__).parent / "templates")


def display_name(prenom: str | None, nom: str | None) -> str:
    """Le nom affiché d'une personne. Les deux champs peuvent être vides (voir migration 0005,
    `slug_derive_d_identifiant`) : on n'invente jamais un nom, on le dit explicitement.
    """
    parts = [part for part in (prenom, nom) if part]
    return " ".join(parts) if parts else "nom inconnu"


# jinja2 type ses globals par défaut sur le dict littéral de `DEFAULT_NAMESPACE`
# (range/dict/lipsum/...), pas sur `dict[str, Any]` : un unique cast justifié ici, plutôt qu'un
# par affectation.
_env_globals = cast(dict[str, Any], templates.env.globals)
_env_globals["thumbnail_url"] = thumbnail_url
_env_globals["candidate_statut_label"] = candidate_statut_label
_env_globals["display_name"] = display_name
_env_globals["NON_INSCRIT_LABEL"] = NON_INSCRIT_LABEL
_env_globals["PISTE_TITLES"] = PISTE_TITLES
_env_globals["vote_position_label"] = vote_position_label
_env_globals["PAR_DELEGATION_LABEL"] = PAR_DELEGATION_LABEL
_env_globals["CONGRES_LABEL"] = CONGRES_LABEL
_env_globals["mise_au_point_label"] = mise_au_point_label
_env_globals["MISE_AU_POINT_RESULTAT_INCHANGE"] = MISE_AU_POINT_RESULTAT_INCHANGE
_env_globals["categorisation_position_label"] = categorisation_position_label
_env_globals["categorisation_method_label"] = categorisation_method_label
_env_globals["orientation_label"] = orientation_label
_env_globals["orientation_preuve_label"] = orientation_preuve_label
_env_globals["orientation_insuffisant_label"] = orientation_insuffisant_label
_env_globals["cohesion_label"] = cohesion_label
_env_globals["NO_ORIENTATIONS_NEVER_SAT_LABEL"] = NO_ORIENTATIONS_NEVER_SAT_LABEL
_env_globals["NO_ORIENTATIONS_UNCATEGORIZED_LABEL"] = NO_ORIENTATIONS_UNCATEGORIZED_LABEL
_env_globals["themes_masques_label"] = themes_masques_label
_env_globals["ecartes_desaccord_label"] = ecartes_desaccord_label
