-- Phase 4.1 — F1 : `person_theme_score_courant` (migration 0010) doit exposer
-- `ecartes_desaccord` (migration 0011), pour que la fiche publique affiche le nombre de
-- contributions écartées par désaccord de mesure à côté de chaque orientation concernée.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).
--
-- Postgres ne permet pas d'ajouter une colonne à une vue matérialisée existante par ALTER : on la
-- recrée. `DROP MATERIALIZED VIEW` entraîne la suppression de ses index, recréés ci-dessous à
-- l'identique.

DROP MATERIALIZED VIEW person_theme_score_courant;

CREATE MATERIALIZED VIEW person_theme_score_courant AS
SELECT ptc.person_id, ptc.theme_id, ptc.score, ptc.incertitude, ptc.contributions, ptc.abstentions,
       ptc.relues, ptc.ecartes_desaccord
FROM person_theme_score ptc
JOIN score_run sr ON sr.id = ptc.run_id AND sr.is_current;

-- Unique et non partiel : REFRESH ... CONCURRENTLY l'exige (même contrainte qu'en migration 0005).
CREATE UNIQUE INDEX person_theme_score_courant_pk_idx ON person_theme_score_courant (person_id, theme_id);
CREATE INDEX person_theme_score_courant_theme_idx ON person_theme_score_courant (theme_id);
