# db/

Schéma PostgreSQL : migrations SQL versionnées dans `migrations/`, nommées `NNNN_description.sql`.

Règles :

- une migration mergée est **immuable** — on corrige avec une nouvelle migration ;
- toute migration doit être rejouable sur une base contenant déjà des données ;
- pas d'ORM propriétaire du schéma : le SQL est la référence, Python et Rust s'y conforment.
