"""Validation sémantique d'un import et calcul de son aperçu — voir
docs/plans/phase-3-categorisation.md, sections « Import : validation, aperçu, application » et
« ⚠️ À concevoir ici, pas ailleurs ». Sépare de `labels_io` (validation de forme, pure) : chaque
règle ici a besoin de la base pour être vérifiée (scrutin_uid connu, thème actif, conflit avec une
catégorisation relue).

Granularité du refus (D3.15) : le fichier entier ou rien. `build_plan` s'arrête et renvoie ses
erreurs dès qu'une passe de vérification en trouve — jamais d'aperçu calculé sur des lignes
partiellement valides.
"""

from collections import defaultdict
from dataclasses import asdict, dataclass, field
from decimal import Context, Decimal, Inexact, InvalidOperation, localcontext

from kyc_api.db import Queryable
from kyc_api.labels_io import ImportRow, ParsedImport
from kyc_api.queries import imports as imports_queries

MAX_ERRORS_REPORTED = 50
JUSTIFICATION_MIN_LENGTH = 10
_ZERO = Decimal("0")
_ONE = Decimal("1")
_NEG_ONE = Decimal("-1")
_QUANTUM = Decimal("0.001")


@dataclass
class ImportProblem:
    source: str
    message: str


@dataclass
class ScrutinLabelData:
    """Comparée par égalité de champs (générée par `@dataclass`) : c'est exactement ce qui décide
    si un scrutin a changé (avant == apres -> « inchangé », voir `build_plan`)."""

    theme: str
    poids: str
    position_pour: str | None
    confiance: str
    justification: str


@dataclass
class ScrutinPlan:
    scrutin_uid: str
    scrutin_id: int
    classement: str  # "creation" | "modification" | "inchange" | "conflit"
    avant: list[ScrutinLabelData]
    apres: list[ScrutinLabelData]


@dataclass
class ImportPlan:
    problems: list[ImportProblem] = field(default_factory=list)
    problems_total: int = 0
    scrutins: list[ScrutinPlan] = field(default_factory=list)

    @property
    def is_valid(self) -> bool:
        return not self.problems

    @property
    def counts(self) -> dict[str, int]:
        counts = {"creation": 0, "modification": 0, "inchange": 0, "conflit": 0}
        for scrutin in self.scrutins:
            counts[scrutin.classement] += 1
        return counts


def _quantize_exact(raw: str) -> Decimal:
    """Trois décimales exactement (D3.13) : `decimal.Inexact` signale une valeur qui en portait
    davantage — le symptôme d'un calcul flottant en amont que le plan demande de nommer."""
    value = Decimal(raw)
    with localcontext(Context(traps=[Inexact])):
        return value.quantize(_QUANTUM)


async def build_plan(pool: Queryable, parsed: ParsedImport) -> ImportPlan:
    problems: list[ImportProblem] = []

    # 1. Couple (scrutin, thème) présent deux fois dans le fichier.
    seen: dict[tuple[str, str], str] = {}
    for row in parsed.rows:
        key = (row.scrutin_uid, row.theme)
        if key in seen:
            problems.append(
                ImportProblem(
                    row.source,
                    f"couple (scrutin {row.scrutin_uid}, thème {row.theme}) déjà présent "
                    f"({seen[key]})",
                )
            )
        else:
            seen[key] = row.source

    # 2. Thèmes : connus et actifs.
    active_theme_slugs = await imports_queries.get_active_theme_slugs(pool)
    theme_has_axis = await imports_queries.theme_has_axis_by_slug(pool)
    valid_slugs_listed = ", ".join(sorted(active_theme_slugs))
    for row in parsed.rows:
        if row.theme not in theme_has_axis:
            problems.append(
                ImportProblem(
                    row.source, f"thème inconnu : {row.theme!r} (valides : {valid_slugs_listed})"
                )
            )
        elif row.theme not in active_theme_slugs:
            problems.append(
                ImportProblem(
                    row.source, f"thème inactif : {row.theme!r} (valides : {valid_slugs_listed})"
                )
            )

    # 3. Valeurs numériques et justification — seulement pour les thèmes reconnus, pour ne pas
    # empiler une seconde erreur sans intérêt sur une ligne déjà rejetée à l'étape précédente.
    for row in parsed.rows:
        has_axis = theme_has_axis.get(row.theme)
        if has_axis is None:
            continue

        problems.extend(_validate_poids(row))
        problems.extend(_validate_confiance(row))
        problems.extend(_validate_position(row, has_axis=has_axis))

        if len(row.justification.strip()) < JUSTIFICATION_MIN_LENGTH:
            problems.append(
                ImportProblem(
                    row.source,
                    f"justification trop courte pour {row.scrutin_uid} "
                    f"({JUSTIFICATION_MIN_LENGTH} caractères minimum)",
                )
            )

    # 4. scrutin_uid existant.
    uids = sorted({row.scrutin_uid for row in parsed.rows})
    scrutin_ids_by_uid = await imports_queries.resolve_scrutin_ids(pool, uids)
    for row in parsed.rows:
        if row.scrutin_uid not in scrutin_ids_by_uid:
            problems.append(ImportProblem(row.source, f"scrutin_uid inconnu : {row.scrutin_uid!r}"))

    if problems:
        return ImportPlan(
            problems=problems[:MAX_ERRORS_REPORTED], problems_total=len(problems), scrutins=[]
        )

    # 5. Somme des poids d'un scrutin exactement égale à 1 — seulement calculable une fois chaque
    # poids individuellement valide (étape 3 déjà passée sans erreur).
    rows_by_uid: dict[str, list[ImportRow]] = defaultdict(list)
    for row in parsed.rows:
        rows_by_uid[row.scrutin_uid].append(row)
    for uid, rows in rows_by_uid.items():
        total = sum((Decimal(r.poids) for r in rows), start=_ZERO)
        if total != _ONE:
            problems.append(
                ImportProblem(f"scrutin {uid}", f"somme des poids = {total}, attendu exactement 1")
            )

    if problems:
        return ImportPlan(
            problems=problems[:MAX_ERRORS_REPORTED], problems_total=len(problems), scrutins=[]
        )

    # Tout est valide : calcul de l'aperçu (D3.16 — un scrutin absent du fichier n'apparaît nulle
    # part ici, donc n'est jamais touché plus tard par l'application).
    scrutin_ids = sorted(set(scrutin_ids_by_uid.values()))
    existing = await imports_queries.get_existing_labels(pool, scrutin_ids)
    existing_by_scrutin_id: dict[int, list] = defaultdict(list)
    for entry in existing:
        existing_by_scrutin_id[entry.scrutin_id].append(entry)

    scrutins: list[ScrutinPlan] = []
    for uid, rows in rows_by_uid.items():
        scrutin_id = scrutin_ids_by_uid[uid]
        avant_raw = existing_by_scrutin_id.get(scrutin_id, [])
        avant = sorted(
            (
                ScrutinLabelData(
                    theme=e.theme_slug,
                    poids=e.poids,
                    position_pour=e.position_pour,
                    confiance=e.confiance,
                    justification=e.justification,
                )
                for e in avant_raw
            ),
            key=lambda d: d.theme,
        )
        apres = sorted(
            (
                ScrutinLabelData(
                    theme=r.theme,
                    poids=r.poids,
                    position_pour=r.position_pour,
                    confiance=r.confiance,
                    justification=r.justification,
                )
                for r in rows
            ),
            key=lambda d: d.theme,
        )

        is_reviewed_source = any(
            e.method == "manual" or e.reviewed_at is not None for e in avant_raw
        )
        if avant == apres:
            classement = "inchange"
        elif not avant_raw:
            classement = "creation"
        elif is_reviewed_source:
            classement = "conflit"
        else:
            classement = "modification"

        scrutins.append(
            ScrutinPlan(
                scrutin_uid=uid,
                scrutin_id=scrutin_id,
                classement=classement,
                avant=avant,
                apres=apres,
            )
        )

    order = {"conflit": 0, "creation": 1, "modification": 2, "inchange": 3}
    scrutins.sort(key=lambda s: (order[s.classement], s.scrutin_uid))

    return ImportPlan(problems=[], problems_total=0, scrutins=scrutins)


def _validate_poids(row: ImportRow) -> list[ImportProblem]:
    try:
        value = _quantize_exact(row.poids)
    except InvalidOperation:
        return [ImportProblem(row.source, f"poids invalide : {row.poids!r}")]
    except Inexact:
        return [ImportProblem(row.source, f"poids à plus de trois décimales : {row.poids!r}")]
    if not (_ZERO < value <= _ONE):
        return [ImportProblem(row.source, f"poids hors bornes ]0, 1] : {row.poids!r}")]
    return []


def _validate_confiance(row: ImportRow) -> list[ImportProblem]:
    try:
        value = _quantize_exact(row.confiance)
    except InvalidOperation:
        return [ImportProblem(row.source, f"confiance invalide : {row.confiance!r}")]
    except Inexact:
        return [
            ImportProblem(row.source, f"confiance à plus de trois décimales : {row.confiance!r}")
        ]
    if not (_ZERO <= value <= _ONE):
        return [ImportProblem(row.source, f"confiance hors bornes [0, 1] : {row.confiance!r}")]
    return []


def _validate_position(row: ImportRow, *, has_axis: bool) -> list[ImportProblem]:
    if not has_axis:
        if row.position_pour is not None:
            return [
                ImportProblem(
                    row.source,
                    f"position renseignée sur {row.scrutin_uid} pour un thème sans axe "
                    f"({row.theme!r})",
                )
            ]
        return []

    if row.position_pour is None:
        return [
            ImportProblem(
                row.source,
                f"position absente sur {row.scrutin_uid} pour un thème à axe ({row.theme!r})",
            )
        ]
    try:
        value = _quantize_exact(row.position_pour)
    except InvalidOperation:
        return [ImportProblem(row.source, f"position invalide : {row.position_pour!r}")]
    except Inexact:
        return [
            ImportProblem(row.source, f"position à plus de trois décimales : {row.position_pour!r}")
        ]
    if not (_NEG_ONE <= value <= _ONE):
        return [ImportProblem(row.source, f"position hors bornes [-1, 1] : {row.position_pour!r}")]
    return []


def plan_to_dict(plan: ImportPlan) -> dict:
    """La forme stockée dans `label_import.apercu` — voir `queries.imports.create_label_import`.
    Recalculée et comparée à l'application (commit suivant, D3.14 étape 2) : c'est la garde contre
    deux admins qui travaillent sur le même import en même temps.
    """
    return {
        "compteurs": plan.counts,
        "scrutins": [
            {
                "scrutin_uid": s.scrutin_uid,
                "scrutin_id": s.scrutin_id,
                "classement": s.classement,
                "avant": [asdict(d) for d in s.avant],
                "apres": [asdict(d) for d in s.apres],
            }
            for s in plan.scrutins
        ],
    }
