from pathlib import Path
from typing import Any, cast

from fastapi.templating import Jinja2Templates

from kyc_api.labels import (
    CONGRES_LABEL,
    NON_INSCRIT_LABEL,
    PAR_DELEGATION_LABEL,
    PISTE_TITLES,
    candidate_statut_label,
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
