.PHONY: dev lint typecheck test migrate db-up ingest css

DATABASE_URL ?= postgresql://kyc:kyc@localhost:5432/kyc

CSS_DIR := backend/src/kyc_api/static/css
CSS_OUTPUT := backend/src/kyc_api/static/style.css
# Tri lexicographique : les préfixes numériques (00-, 10-, ..., 50-) ont tous deux chiffres, donc
# l'ordre lexicographique est l'ordre de cascade voulu (D2.6).
CSS_SOURCES := $(sort $(wildcard $(CSS_DIR)/*.css))

# `podman compose up -d` rend la main avant que Postgres accepte les connexions ; au premier
# démarrage à froid, ça fait échouer `make migrate` en « connection refused ». `--wait` aide mais
# son support varie selon les versions de podman-compose, donc on ne s'y fie pas seul : on fait
# suivre d'une boucle d'attente bornée sur `pg_isready` (voir F6, docs/plans/phase-0.1-fix.md).
#
# `-h 127.0.0.1` force une vérification TCP : pendant l'initialisation de la base, l'entrypoint de
# l'image postgres démarre un serveur temporaire avec `listen_addresses=''`, qui répond sur la
# socket Unix mais pas en TCP. Sans ce drapeau, la boucle peut réussir alors que le port publié
# refuse encore les connexions — soit la panne que cette cible est censée éliminer.
db-up:
	podman compose up -d --wait 2>/dev/null || podman compose up -d
	@echo "Attente de PostgreSQL..."
	@for i in $$(seq 1 30); do \
		podman compose exec -T postgres pg_isready -U kyc -d kyc -h 127.0.0.1 >/dev/null 2>&1 && exit 0; \
		sleep 1; \
	done; \
	echo "PostgreSQL ne répond pas après 30 s. Vérifier 'podman compose logs postgres'." >&2; \
	exit 1

dev: db-up css
	$(MAKE) migrate
	@trap 'kill 0' EXIT; \
	(cd worker && cargo run) & \
	(cd backend && uv run uvicorn kyc_api.main:app --reload --port 8000) & \
	wait

# `style.css` est GÉNÉRÉ (voir sa bannière) et commité : `make lint` le régénère puis vérifie que
# rien n'a changé, sinon la CI ne peut jamais être verte avec une feuille périmée (D2.6). Personne
# n'a besoin de se souvenir de lancer `make css` avant de committer.
css:
	@{ \
		echo '/* FICHIER GÉNÉRÉ par `make css` — ne pas éditer directement, éditer $(CSS_DIR)/*.css */'; \
		echo; \
		cat $(CSS_SOURCES); \
	} > $(CSS_OUTPUT)

lint: css
	git diff --exit-code -- $(CSS_OUTPUT)
	cd backend && uv run ruff check . && uv run ruff format --check .
	cd worker && cargo fmt --check && cargo clippy --all-targets -- -D warnings

typecheck:
	cd backend && uv run ty check

test: db-up
	cd backend && uv run pytest
	cd worker && SQLX_OFFLINE=true cargo test

migrate:
	sqlx migrate run --source db/migrations --database-url "$(DATABASE_URL)"

# Enchaîne le référentiel puis les trois législatures dans l'ordre (D. spike : ingest_acteurs
# avant ingest_scrutins, voir docs/plans/phase-1-ingestion.md), puis enrich_wikidata (a besoin des
# `person` du référentiel, F2c docs/plans/phase-1.1-fix.md), puis vide la file en une seule
# exécution (`run-once`) plutôt que de laisser un worker tourner indéfiniment derrière. Compte en
# dizaines de minutes sur les trois législatures complètes — c'est attendu, pas un hang. `run-once`
# sort en code non nul si l'un des jobs échoue (F3) : `make ingest` s'arrête donc avec lui.
ingest: db-up
	$(MAKE) migrate
	cd worker && cargo run --release -- enqueue ingest_acteurs
	cd worker && cargo run --release -- enqueue ingest_scrutins '{"legislature": 15}'
	cd worker && cargo run --release -- enqueue ingest_scrutins '{"legislature": 16}'
	cd worker && cargo run --release -- enqueue ingest_scrutins '{"legislature": 17}'
	cd worker && cargo run --release -- enqueue enrich_wikidata
	cd worker && cargo run --release -- enqueue seed_candidates
	cd worker && cargo run --release -- enqueue assign_slugs
	cd worker && cargo run --release -- enqueue seed_themes
	cd worker && cargo run --release -- enqueue label_scrutins_heuristic
	cd worker && cargo run --release -- enqueue refresh_views
	cd worker && cargo run --release -- run-once
