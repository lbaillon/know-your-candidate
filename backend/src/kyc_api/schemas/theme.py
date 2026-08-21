"""Modèles Pydantic des thèmes et de leur axe — voir docs/plans/phase-3-categorisation.md, D3.12
(thème et axe fusionnés)."""

from pydantic import BaseModel


class Theme(BaseModel):
    id: int
    slug: str
    libelle: str
    description: str
    libelle_pole_negatif: str | None
    libelle_pole_positif: str | None
    rang: int
    # F2, docs/plans/phase-4.1-partis-scores.md : un thème dont l'axe ne se lit pas gauche-droite
    # (institutions/démocratie, agriculture, Europe) reste consultable ici — la catégorisation des
    # scrutins n'est pas en cause — mais ne produit aucune orientation publique tant qu'aucune
    # relecture humaine ne lui donne un fondement (voir queries/scores.py).
    axe_gauche_droite: bool

    @property
    def has_axis(self) -> bool:
        return self.libelle_pole_negatif is not None
