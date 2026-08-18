-- Phase 2.1 — F5 + F6 : recherche insensible aux accents, index morts retirés.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).

-- 1. Recherche insensible aux accents (F5, D2.15) ---------------------------------------------
--
-- `ILIKE '%melenchon%'` ne trouve rien : la colonne comparée porte les accents, la requête n'en a
-- pas. Sur un site français, c'est la majorité des recherches qui échouent. `unaccent()` est
-- STABLE et non IMMUTABLE (son dictionnaire peut être rechargé), donc inutilisable telle quelle
-- dans un index. Fixer le dictionnaire explicitement lève l'ambiguïté et rend l'enveloppe
-- déclarable IMMUTABLE : c'est la forme recommandée par la documentation PostgreSQL.

CREATE EXTENSION IF NOT EXISTS unaccent;

CREATE FUNCTION kyc_unaccent(text) RETURNS text
    AS $$ SELECT public.unaccent('public.unaccent'::regdictionary, $1) $$
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE;

-- 2. `person_apercu` avec une colonne de recherche normalisée -----------------------------------
--
-- PostgreSQL ne connaît pas `CREATE OR REPLACE MATERIALIZED VIEW` : on la recrée. Définition de la
-- migration 0005 recopiée mot pour mot, seule la colonne `recherche` est ajoutée — ce n'est pas
-- l'occasion de l'améliorer, toute divergence involontaire serait invisible.

DROP MATERIALIZED VIEW person_apercu;

CREATE MATERIALIZED VIEW person_apercu AS
SELECT p.id                                            AS person_id,
       s.slug,
       p.civilite, p.prenom, p.nom, p.date_deces,
       ph.url                                          AS photo_url,
       ph.commons_file, ph.licence, ph.licence_url, ph.auteur,
       coalesce(v.votes_total, 0)                      AS votes_total,
       v.premier_vote,
       v.dernier_vote,
       g.organe_id                                     AS dernier_groupe_id,
       g.libelle                                       AS dernier_groupe_libelle,
       g.libelle_abrege                                AS dernier_groupe_abrege,
       g.is_non_inscrit                                AS dernier_groupe_non_inscrit,
       g.period                                         AS dernier_groupe_period,
       coalesce(l.legislatures, '{}')                  AS legislatures,
       (c.person_id IS NOT NULL)                       AS est_candidat,
       kyc_unaccent(lower(coalesce(p.prenom, '') || ' ' || coalesce(p.nom, ''))) AS recherche
FROM person p
LEFT JOIN person_slug s ON s.person_id = p.id AND s.is_current
LEFT JOIN person_photo ph ON ph.person_id = p.id
LEFT JOIN candidate c ON c.person_id = p.id
LEFT JOIN LATERAL (
    SELECT count(*) AS votes_total,
           min(sc.date_scrutin) AS premier_vote,
           max(sc.date_scrutin) AS dernier_vote
    FROM vote v
    JOIN scrutin sc ON sc.id = v.scrutin_id
    WHERE v.person_id = p.id AND sc.chambre = 'assemblee'
) v ON true
LEFT JOIN LATERAL (
    SELECT o.id AS organe_id, o.libelle, o.libelle_abrege, o.is_non_inscrit, m.period
    FROM mandat m
    JOIN organe o ON o.id = m.organe_id
    WHERE m.person_id = p.id AND m.type_organe = 'GP'
    ORDER BY upper(m.period) DESC NULLS FIRST, lower(m.period) DESC, m.id DESC
    LIMIT 1
) g ON true
LEFT JOIN LATERAL (
    SELECT array_agg(DISTINCT m.legislature ORDER BY m.legislature) AS legislatures
    FROM mandat m
    WHERE m.person_id = p.id AND m.type_organe = 'ASSEMBLEE' AND m.legislature IS NOT NULL
) l ON true;

-- Unique et non partiel : REFRESH ... CONCURRENTLY l'exige.
CREATE UNIQUE INDEX person_apercu_pk_idx   ON person_apercu (person_id);
CREATE UNIQUE INDEX person_apercu_slug_idx ON person_apercu (slug);
CREATE INDEX person_apercu_votes_idx       ON person_apercu (votes_total DESC);
CREATE INDEX person_apercu_recherche_idx   ON person_apercu USING gin (recherche gin_trgm_ops);

-- 3. Index morts retirés (F6) --------------------------------------------------------------------
--
-- `person_nom_trgm_idx` (0002) et `person_recherche_trgm_idx` (0005) sont posés sur `person`, alors
-- que toutes les recherches interrogent `person_apercu` — `EXPLAIN` confirme un Seq Scan, jamais un
-- usage de ces index. Le commentaire de la migration 0005 affirmait à tort que le second servait la
-- recherche ; il ne servait rien. L'index utile est `person_apercu_recherche_idx` ci-dessus, posé là
-- où la requête regarde réellement.

DROP INDEX person_recherche_trgm_idx;
DROP INDEX person_nom_trgm_idx;
