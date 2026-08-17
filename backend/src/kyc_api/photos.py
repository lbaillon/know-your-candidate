"""URL de vignette Wikimedia Commons dérivée de `commons_file` (D2.9) — voir
docs/plans/phase-2-api-ui.md. `Special:FilePath` redirige vers le fichier redimensionné : point
d'entrée documenté et stable, pas besoin de connaître le chemin de hachage MD5 de Commons.
"""

from urllib.parse import quote

_THUMBNAIL_WIDTH = 400


def thumbnail_url(commons_file: str | None) -> str | None:
    """`None` si `commons_file` est absent ou vide : le gabarit affiche alors un cadre de repli,
    jamais un `<img>` cassé.
    """
    if commons_file is None:
        return None
    filename = commons_file.strip()
    if not filename:
        return None
    encoded = quote(filename.replace(" ", "_"), safe="")
    return f"https://commons.wikimedia.org/wiki/Special:FilePath/{encoded}?width={_THUMBNAIL_WIDTH}"
