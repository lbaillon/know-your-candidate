"""Rendu des documents Markdown du dépôt — voir docs/plans/phase-2-api-ui.md, livrables
`/methodologie` et `/sources`. Les deux pages servent le texte de docs/methodology.md et
docs/data-sources.md : un seul contenu à maintenir, jamais une copie qui pourrait diverger.
"""

from pathlib import Path

import markdown

# backend/src/kyc_api/documents.py -> repo root : même profondeur que config.py (voir son
# commentaire sur _REPO_ROOT_ENV) pour la même raison, indépendant du cwd du process.
_DOCS_DIR = Path(__file__).resolve().parents[3] / "docs"

_EXTENSIONS = ["tables"]


def render_document(relative_path: str) -> str:
    """Rend un document Markdown du dépôt en HTML. `relative_path` est relatif à `docs/` et
    toujours une constante codée en dur par ses deux seuls appelants (jamais une entrée de
    requête) : aucune validation de traversée de chemin n'est donc nécessaire ici.
    """
    text = (_DOCS_DIR / relative_path).read_text(encoding="utf-8")
    return markdown.markdown(text, extensions=_EXTENSIONS)
