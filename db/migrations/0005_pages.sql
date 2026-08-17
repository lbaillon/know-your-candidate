-- Phase 2 — pages publiques : identité éditoriale des candidat·es, slugs stables, aperçus
-- pré-calculés pour l'accueil et l'annuaire.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).

-- 1. Une personne peut n'avoir jamais siégé -------------------------------------------------------
--
-- La phase 1 supposait que toute personne vient d'AMO30, d'où `an_uid NOT NULL`. C'est faux pour une
-- candidate qui n'a jamais été députée (methodology.md § 7.4 : « beaucoup de candidat·es n'ont jamais
-- siégé »). On lève la contrainte plutôt que de dupliquer l'identité dans une seconde table : une
-- personne reste une personne, seule sa provenance change.

ALTER TABLE person ALTER COLUMN an_uid DROP NOT NULL;

ALTER TABLE person ADD CONSTRAINT person_a_au_moins_un_identifiant
    CHECK (an_uid IS NOT NULL OR wikidata_qid IS NOT NULL);

-- 2. Le statut de candidat·e, éditorial et sourcé --------------------------------------------------
--
-- `source_url` et `source_date` sont NOT NULL par construction, sur le modèle de
-- `person_photo.licence` : la règle « une donnée sans source affichable n'entre pas en base »
-- appartient au schéma, pas au code applicatif qui pourrait l'oublier.

CREATE TYPE candidate_statut AS ENUM ('declare', 'pressenti', 'retire');

CREATE TABLE candidate (
    person_id   bigint PRIMARY KEY REFERENCES person (id) ON DELETE CASCADE,
    statut      candidate_statut NOT NULL,
    source_url  text NOT NULL,
    source_date date NOT NULL,
    note        text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT candidate_source_non_vide CHECK (btrim(source_url) <> '')
);

-- 3. Slugs historisés (D2.3) -----------------------------------------------------------------------
--
-- Le slug est la clé : une ligne par slug jamais attribué deux fois. `is_current` distingue le slug
-- servi du ou des anciens, qui restent pour rediriger en 301. L'index partiel garantit qu'une
-- personne n'a jamais deux slugs courants.

CREATE TABLE person_slug (
    slug       text PRIMARY KEY,
    person_id  bigint NOT NULL REFERENCES person (id) ON DELETE CASCADE,
    is_current boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX person_slug_courant_idx ON person_slug (person_id) WHERE is_current;

-- 4. Recherche par nom complet ---------------------------------------------------------------------
--
-- `person_nom_trgm_idx` (migration 0002) ne couvre que `nom` : taper « jean-luc mélenchon » ne
-- trouve rien. pg_trgm est déjà activé par la migration 0001.

CREATE INDEX person_recherche_trgm_idx ON person
    USING gin ((coalesce(prenom, '') || ' ' || coalesce(nom, '')) gin_trgm_ops);

-- 5. Aperçu par personne ---------------------------------------------------------------------------
--
-- L'accueil et l'annuaire ont besoin, par personne, du nombre de votes et du dernier groupe connu.
-- Compter les votes de 1 900 personnes dans une table de 2,3 M de lignes à chaque affichage est
-- exactement ce que l'architecture interdit au backend : le calcul devient donc une vue
-- matérialisée, rafraîchie par le worker (architecture.md § 1).
--
-- « Dernier groupe connu » et non « groupe actuel » : un `CURRENT_DATE` dans une vue matérialisée
-- est figé à l'instant du rafraîchissement et se met à mentir en silence. On classe par fin de
-- mandat, ce qui ne dépend pas de la date de calcul, et l'UI affiche la période plutôt que d'affirmer
-- un présent.
--
-- Les compteurs excluent le Congrès (methodology.md, cas VTCGR5L16V1) : le chiffre affiché dit
-- « votes à l'Assemblée », il doit donc être exactement cela. Le vote du Congrès reste visible dans
-- la liste des votes, signalé comme tel.

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
       g.period                                        AS dernier_groupe_period,
       coalesce(l.legislatures, '{}')                  AS legislatures,
       (c.person_id IS NOT NULL)                       AS est_candidat
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
CREATE UNIQUE INDEX person_apercu_pk_idx ON person_apercu (person_id);
CREATE UNIQUE INDEX person_apercu_slug_idx ON person_apercu (slug);
CREATE INDEX person_apercu_votes_idx ON person_apercu (votes_total DESC);
