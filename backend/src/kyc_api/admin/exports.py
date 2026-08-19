"""GET /admin/export — voir docs/plans/phase-3-categorisation.md, section « Le cycle export /
import ». Assemble les scrutins demandés via `queries.exports`, les thèmes actifs, et les sérialise
avec `labels_io` (fonctions pures, aucune requête là-dedans).
"""

from datetime import UTC, datetime

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.responses import PlainTextResponse

from kyc_api import labels_io
from kyc_api.admin.auth import require_admin
from kyc_api.db import Queryable, get_pool
from kyc_api.queries import exports as exports_queries
from kyc_api.queries import themes as themes_queries
from kyc_api.schemas.admin import AdminUser
from kyc_api.schemas.import_ import ExportFile, ExportFiltre, ExportTheme

router = APIRouter()

_ALLOWED_STATUTS = ("non_categorises", "categorises", "tous")
_ALLOWED_FORMATS = ("csv", "json")


@router.get("/export")
async def export_labels(
    statut: str = Query("non_categorises"),
    format: str = Query("csv"),
    theme: str | None = Query(None),
    limite: int | None = Query(None),
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
) -> PlainTextResponse:
    if statut not in _ALLOWED_STATUTS:
        raise HTTPException(status_code=400, detail=f"statut inconnu : {statut!r}")
    if format not in _ALLOWED_FORMATS:
        raise HTTPException(status_code=400, detail=f"format inconnu : {format!r}")

    active_themes = await themes_queries.list_active(pool)
    scrutins = await exports_queries.list_scrutins_for_export(
        pool, statut=statut, theme_slug=theme, limite=limite
    )

    export = ExportFile(
        generated_at=datetime.now(UTC),
        filtre=ExportFiltre(statut=statut, theme=theme),
        themes=[
            ExportTheme(
                slug=t.slug,
                libelle=t.libelle,
                pole_negatif=t.libelle_pole_negatif,
                pole_positif=t.libelle_pole_positif,
            )
            for t in active_themes
        ],
        scrutins=scrutins,
    )

    if format == "json":
        content = labels_io.write_json(export)
        media_type = "application/json"
        extension = "json"
    else:
        content = labels_io.write_csv(export)
        media_type = "text/csv"
        extension = "csv"

    filename = f"categorisation-{statut}-{export.generated_at:%Y%m%d}.{extension}"
    return PlainTextResponse(
        content,
        media_type=media_type,
        headers={"Content-Disposition": f'attachment; filename="{filename}"'},
    )
