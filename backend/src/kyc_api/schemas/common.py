"""Modèles Pydantic partagés par plusieurs objets — voir docs/plans/phase-2-api-ui.md."""

from pydantic import BaseModel


class Pagination(BaseModel):
    """Pagination par numéro de page (l'annuaire) — distincte de la pagination par curseur des
    votes d'une personne (D2.8, section « Liste des votes »), qui a ses propres champs.
    """

    page: int
    page_size: int
    total: int

    @property
    def total_pages(self) -> int:
        if self.total == 0:
            return 0
        return -(-self.total // self.page_size)  # division entière arrondie au supérieur

    @property
    def has_next(self) -> bool:
        return self.page < self.total_pages

    @property
    def has_previous(self) -> bool:
        return self.page > 1
