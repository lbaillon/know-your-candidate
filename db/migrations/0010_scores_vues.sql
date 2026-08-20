-- Phase 4 — vues matérialisées de lecture : person_theme_score_courant, mandat_theme_score_courant.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).
--
-- Restreintes au run courant (`score_run.is_current`), rafraîchies `CONCURRENTLY` par
-- `recompute_scores` juste après la bascule de `is_current` — même piège qu'en phase 2 (migration
-- 0005) : `REFRESH ... CONCURRENTLY` ne s'exécute pas dans une transaction, les tests rafraîchissent
-- donc sans `CONCURRENTLY`. Pas de vue équivalente pour `groupe_theme_score` : sa table est petite
-- (un organe GP par législature) et sa clé primaire commence déjà par `run_id`, la jointure directe
-- à `score_run` reste une poignée de requêtes indexées (architecture.md § 1).

CREATE MATERIALIZED VIEW person_theme_score_courant AS
SELECT ptc.person_id, ptc.theme_id, ptc.score, ptc.incertitude, ptc.contributions, ptc.abstentions,
       ptc.relues
FROM person_theme_score ptc
JOIN score_run sr ON sr.id = ptc.run_id AND sr.is_current;

-- Unique et non partiel : REFRESH ... CONCURRENTLY l'exige (même contrainte qu'en migration 0005).
CREATE UNIQUE INDEX person_theme_score_courant_pk_idx ON person_theme_score_courant (person_id, theme_id);
CREATE INDEX person_theme_score_courant_theme_idx ON person_theme_score_courant (theme_id);

CREATE MATERIALIZED VIEW mandat_theme_score_courant AS
SELECT mts.mandat_id, mts.theme_id, mts.score, mts.cohesion, mts.contributions
FROM mandat_theme_score mts
JOIN score_run sr ON sr.id = mts.run_id AND sr.is_current;

CREATE UNIQUE INDEX mandat_theme_score_courant_pk_idx ON mandat_theme_score_courant (mandat_id, theme_id);
