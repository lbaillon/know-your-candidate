-- Phase 4 — scores : bipolarité des scrutins, paramètres, exécutions, et les quatre tables de
-- score (personne, groupe, mandat, contributions).
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).

-- 1. La mesure qui manquait à la phase 3 (D4.9) ---------------------------------------------------
--
-- `separation` dit si un seuil sépare bien les deux camps ; elle ne dit pas si le camp « contre »
-- est idéologiquement homogène. Sur un budget, l'opposition vient des deux extrêmes à la fois : le
-- vote ne situe personne, et la phase 3 l'a payé (F9, phase-3.0-feedback.md). `bipolarite` mesure
-- exactement cela, et entre dans la pondération en facteur (1 − bipolarite).
--
-- NULL est permis : les estimations calculées avant cette migration n'en ont pas, et le job les
-- recalculera. Un score ne se calcule que sur des scrutins dont la bipolarité est connue.

ALTER TABLE scrutin_axis_estimate
    ADD COLUMN bipolarite numeric(4,3) CHECK (bipolarite BETWEEN 0 AND 1);

-- 2. Les paramètres du calcul, en base et non en dur ------------------------------------------------
--
-- Même raison qu'en phase 3 pour `corpus_parametre` : déplacer un seuil est une décision éditoriale,
-- elle doit être visible et datée. `formula_version` est incrémentée à la main quand la formule
-- change — c'est elle qui permet de dire pourquoi un chiffre a bougé.

CREATE TABLE score_parametre (
    id                        boolean PRIMARY KEY DEFAULT true CHECK (id),
    contributions_min         smallint NOT NULL DEFAULT 5  CHECK (contributions_min > 0),
    scrutins_min_par_theme    smallint NOT NULL DEFAULT 10 CHECK (scrutins_min_par_theme > 0),
    formula_version           smallint NOT NULL DEFAULT 1,
    updated_at                timestamptz NOT NULL DEFAULT now(),
    updated_by                text
);

INSERT INTO score_parametre (id) VALUES (true);

-- 3. Une exécution de calcul (D4.6) -----------------------------------------------------------------

CREATE TABLE score_run (
    id                     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    formula_version        smallint NOT NULL,
    contributions_min      smallint NOT NULL,
    scrutins_min_par_theme smallint NOT NULL,
    started_at             timestamptz NOT NULL DEFAULT now(),
    finished_at            timestamptz,
    counters               jsonb NOT NULL DEFAULT '{}'::jsonb,
    is_current             boolean NOT NULL DEFAULT false
);

CREATE UNIQUE INDEX score_run_courant_idx ON score_run (is_current) WHERE is_current;

-- 4. Les contributions — la table qui rend l'explication possible -----------------------------------
--
-- architecture.md § 3 : « score_contribution n'est pas un détail d'implémentation : c'est le
-- produit. Sans elle, on affiche un verdict sans preuve, ce que le projet refuse. »
--
-- `apport` est la position sur l'axe que CE vote attribue à la personne : +position_pour si elle a
-- voté pour, −position_pour si elle a voté contre, NULL pour une abstention (D4.10). `poids` est nul
-- pour une abstention : elle est listée, elle ne pèse pas.

CREATE TABLE score_contribution (
    run_id        bigint   NOT NULL REFERENCES score_run (id) ON DELETE CASCADE,
    person_id     bigint   NOT NULL REFERENCES person (id) ON DELETE CASCADE,
    theme_id      smallint NOT NULL REFERENCES theme (id),
    scrutin_id    bigint   NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    position      vote_position NOT NULL,
    apport        numeric(4,3) CHECK (apport BETWEEN -1 AND 1),
    poids         numeric(6,5) NOT NULL CHECK (poids >= 0),
    PRIMARY KEY (run_id, person_id, theme_id, scrutin_id),
    CONSTRAINT score_contribution_abstention_sans_apport
        CHECK ((position = 'abstention') = (apport IS NULL)),
    CONSTRAINT score_contribution_abstention_sans_poids
        CHECK (position <> 'abstention' OR poids = 0)
);

CREATE INDEX score_contribution_lecture_idx
    ON score_contribution (run_id, person_id, theme_id, scrutin_id);

-- 5. Les scores ---------------------------------------------------------------------------------------
--
-- `contributions` compte les votes qui pèsent ; `abstentions` celles qui ne pèsent pas mais qui sont
-- affichées ; `relues` la part des catégorisations sous-jacentes validées par un humain (D4.1) —
-- aujourd'hui zéro partout, et l'UI l'écrit.

CREATE TABLE person_theme_score (
    run_id        bigint   NOT NULL REFERENCES score_run (id) ON DELETE CASCADE,
    person_id     bigint   NOT NULL REFERENCES person (id) ON DELETE CASCADE,
    theme_id      smallint NOT NULL REFERENCES theme (id),
    score         numeric(4,3) NOT NULL CHECK (score BETWEEN -1 AND 1),
    incertitude   numeric(4,3) NOT NULL CHECK (incertitude >= 0),
    contributions integer  NOT NULL CHECK (contributions >= 0),
    abstentions   integer  NOT NULL DEFAULT 0 CHECK (abstentions >= 0),
    relues        integer  NOT NULL DEFAULT 0 CHECK (relues >= 0),
    PRIMARY KEY (run_id, person_id, theme_id)
);

CREATE TABLE groupe_theme_score (
    run_id        bigint   NOT NULL REFERENCES score_run (id) ON DELETE CASCADE,
    organe_id     bigint   NOT NULL REFERENCES organe (id) ON DELETE CASCADE,
    theme_id      smallint NOT NULL REFERENCES theme (id),
    score         numeric(4,3) NOT NULL CHECK (score BETWEEN -1 AND 1),
    cohesion      numeric(4,3) NOT NULL CHECK (cohesion BETWEEN 0 AND 1),
    contributions integer  NOT NULL CHECK (contributions >= 0),
    membres       integer  NOT NULL CHECK (membres >= 0),
    PRIMARY KEY (run_id, organe_id, theme_id)
);

-- Le score du groupe restreint à la période d'appartenance de la personne (D4.12, methodology.md
-- § 6). C'est « tout l'intérêt des daterange » : le même groupe ne dit pas la même chose selon la
-- tranche de temps qu'on regarde.

CREATE TABLE mandat_theme_score (
    run_id        bigint   NOT NULL REFERENCES score_run (id) ON DELETE CASCADE,
    mandat_id     bigint   NOT NULL REFERENCES mandat (id) ON DELETE CASCADE,
    theme_id      smallint NOT NULL REFERENCES theme (id),
    score         numeric(4,3) NOT NULL CHECK (score BETWEEN -1 AND 1),
    cohesion      numeric(4,3) NOT NULL CHECK (cohesion BETWEEN 0 AND 1),
    contributions integer  NOT NULL CHECK (contributions >= 0),
    PRIMARY KEY (run_id, mandat_id, theme_id)
);
