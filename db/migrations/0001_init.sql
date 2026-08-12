-- Phase 0 — socle : extensions, file de jobs, battement de cœur du worker.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md). Toute correction se fait
-- par une nouvelle migration numérotée.

CREATE EXTENSION IF NOT EXISTS btree_gist;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- File de jobs -----------------------------------------------------------------------------------
--
-- Seul canal entre le backend (créateur) et le worker (consommateur unique). Voir la section
-- « PostgreSQL comme unique canal » de docs/plans/phase-0-socle.md.

CREATE TYPE job_status AS ENUM ('pending', 'running', 'done', 'failed', 'cancelled');

CREATE TABLE job (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    type             text        NOT NULL,
    payload          jsonb       NOT NULL DEFAULT '{}'::jsonb,
    status           job_status  NOT NULL DEFAULT 'pending',
    priority         smallint    NOT NULL DEFAULT 0,
    attempts         smallint    NOT NULL DEFAULT 0,
    max_attempts     smallint    NOT NULL DEFAULT 3,
    scheduled_at     timestamptz NOT NULL DEFAULT now(),
    locked_at        timestamptz,
    locked_by        text,
    cancel_requested boolean     NOT NULL DEFAULT false,
    progress         real,                 -- 0.0 à 1.0, NULL si inconnu
    progress_message text,
    error            text,
    created_by       text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    started_at       timestamptz,
    finished_at      timestamptz
);

-- Rend triviale la prise de job : la file « pending » reste petite à l'échelle du projet.
CREATE INDEX job_pending_idx
    ON job (priority DESC, scheduled_at, id)
    WHERE status = 'pending';

-- Battement de cœur du worker ---------------------------------------------------------------------
--
-- Une seule ligne par worker vivant. C'est ce que /healthz lit pour savoir si le worker tourne.

CREATE TABLE worker_heartbeat (
    worker_id      text PRIMARY KEY,
    version        text        NOT NULL,
    started_at     timestamptz NOT NULL DEFAULT now(),
    last_seen_at   timestamptz NOT NULL DEFAULT now(),
    current_job_id bigint REFERENCES job (id) ON DELETE SET NULL
);

-- Ingestion run -------------------------------------------------------------------------------------
--
-- Squelette minimal : la table n'est réellement exploitée qu'à partir de la phase 1 (ingestion de
-- l'open data de l'Assemblée nationale). On la crée ici pour ne pas laisser la migration du socle
-- incomplète.

CREATE TABLE ingestion_run (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source       text        NOT NULL,
    status       text        NOT NULL DEFAULT 'pending',
    counters     jsonb       NOT NULL DEFAULT '{}'::jsonb,
    started_at   timestamptz,
    finished_at  timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- Notification de job --------------------------------------------------------------------------------
--
-- `pg_notify` n'est délivré qu'au COMMIT de la transaction qui l'émet : le worker ne peut donc jamais
-- être réveillé pour un job qu'il ne verrait pas encore en le sélectionnant. Comportement voulu, ne
-- pas chercher à le contourner (voir docs/plans/phase-0-socle.md).

CREATE FUNCTION notify_job() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('kyc_jobs', NEW.id::text);
    RETURN NEW;
END;
$$;

CREATE TRIGGER job_notify
    AFTER INSERT ON job
    FOR EACH ROW WHEN (NEW.status = 'pending')
    EXECUTE FUNCTION notify_job();
