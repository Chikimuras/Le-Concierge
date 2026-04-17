# Le Concierge — Instructions Projet pour Claude

> Coller ce fichier (ou son contenu) dans les **Instructions projet** Claude. C'est le contrat de référence. Toute dérogation doit être justifiée et validée explicitement.

---

## 1. Contexte produit

- **Produit** : *Le Concierge* — plateforme SaaS de **conciergerie immobilière** (gestion Airbnb / locations courte durée).
- **Multi-tenant** : chaque conciergerie = une *organisation* isolée avec ses propres rôles, biens, voyageurs, équipes.
- **Acteurs** : `owner` (propriétaire), `manager` (gérant de conciergerie), `cleaner` (ménage), `guest` (voyageur), `admin` (plateforme).
- **Équipe** : développeur **solo**. Le code doit être maintenable seul, sans effet "usine à gaz".
- **Criticité** : paiements, données personnelles, calendriers synchronisés avec OTAs → **ultra sécurisé non-négociable**.

---

## 2. Stack technique — choix figés

### 2.1 Backend (Rust)

- **Langage** : Rust stable récent (edition 2024).
- **Framework HTTP** : **Axum** (Tokio).
- **Base de données** : **PostgreSQL** (UE, hébergement OVH/Scaleway/Hetzner).
- **Accès données** : **SQLx** (requêtes vérifiées à la compilation, `sqlx::query!` / `query_as!`), **jamais** d'ORM en plus.
- **Migrations** : `sqlx migrate` uniquement, fichiers SQL versionnés dans `apps/api/migrations/`.
- **Cache / sessions** : **Redis**.
- **Messaging / jobs asynchrones** : **RabbitMQ** ou **NATS** (à trancher à la 1ʳᵉ tâche qui en a vraiment besoin — pas avant).
- **API style** : **REST** + **OpenAPI** généré automatiquement via **utoipa** + **utoipa-axum**. Servi par **Scalar** (ou Swagger UI en fallback).
- **Gestion d'erreurs** : `thiserror` dans les modules/libs, `anyhow` uniquement dans `main.rs` / bootstrap. Une enum `AppError` centralisée convertit vers `Response` HTTP (mapping erreur → status code explicite).
- **Sérialisation** : `serde` + `serde_json`.
- **Temps réel** : **WebSocket** (Axum natif) et/ou **SSE** pour notifications (check-in, paiements, messages).
- **Observabilité** : `tracing` + `tracing-subscriber` (format JSON en prod, pretty en dev) → **OpenTelemetry** → **Loki + Grafana + Tempo**.

### 2.2 Frontend (Vue 3)

- **Framework** : **Vue 3** — **Composition API uniquement** (jamais Options API).
- **Language** : **TypeScript strict** (`strict: true`, `noUncheckedIndexedAccess: true`, `noImplicitAny`).
- **Build** : **Vite** + **Bun** (runtime + package manager — `bun install`, `bun run dev`, etc.).
- **UI kit** : **shadcn-vue** (style shadcn/ui porté Vue) + **Tailwind CSS**.
- **State** : **Pinia** (stores typés, composition API).
- **Routing** : **Vue Router** + guards d'auth (redirect non-auth, check rôle par route).
- **Validation** : **vee-validate** + **Zod** — schémas Zod **partagés** avec le backend via un package `packages/contracts/` (types TS générés depuis OpenAPI via `openapi-typescript`).
- **Data fetching** : **TanStack Query (Vue Query)** — jamais de `fetch` manuel dans les composants.
- **HTTP client** : **Ky.js** uniquement (**jamais axios**). Configurer un singleton `apiClient` avec hooks de refresh 2FA/session + retry/backoff.
- **i18n** : **vue-i18n** avec locales `fr` et `en`. Les messages d'erreur *utilisateur* sont **en français** par défaut.
- **Icônes** : `lucide-vue-next`.

### 2.3 Repo & tooling

- **Structure** : **monorepo**.
  ```
  /apps
    /api            # Rust (cargo workspace member)
    /web            # Vue 3 (bun workspace member)
  /packages
    /contracts      # types TS générés depuis OpenAPI + schémas Zod partagés
    /ui             # composants shadcn-vue partagés (si besoin)
  /infra
    /docker         # Dockerfiles, compose, configs
    /grafana        # dashboards provisionnés
  /docs
    /adr            # Architecture Decision Records
  ```
- **Workspaces** : `cargo workspace` côté Rust, `bun workspaces` côté JS.
- **Containerisation** : **API dockerisée** (multi-stage, `distroless` ou `gcr.io/distroless/cc` pour l'image finale, utilisateur non-root, scratch si possible). `docker compose` pour dev local *et* prod (VPS).
- **CI/CD** : **Gitea Actions (Forgejo)** — self-hosted, 100% OSS. Pipeline : `fmt` → `clippy -D warnings` → `cargo test` → `cargo audit` → `cargo deny` → `bun lint` → `bun test` → `bun build` → build images Docker → deploy SSH sur VPS.
- **Git workflow** : **Conventional Commits** + **SemVer**. Pas de merge direct sur `main`. Branches : `feat/...`, `fix/...`, `chore/...`.
- **Pre-commit** : **lefthook** — `rustfmt`, `clippy`, `eslint`, `prettier`, tests rapides.

---

## 3. Sécurité — exigences non-négociables

Pour chaque mesure ci-dessous, **citer la référence** (OWASP ASVS / Top 10, RFC, doc Rust crate) en commentaire ou en commit quand elle est mise en place.

### 3.1 Authentification & sessions
- **Hash mots de passe** : `argon2` (Argon2id) avec paramètres OWASP 2024+ (`m=19456, t=2, p=1` minimum) + **pepper** stocké hors DB (variable d'env / secret Docker).
- **Sessions serveur** stockées dans Redis (jamais en JWT stateless pour ce projet). Cookies `HttpOnly`, `Secure`, `SameSite=Lax` (ou `Strict` quand possible), `Path=/`, domaine scopé.
- **2FA TOTP obligatoire** pour `admin` et `manager`, optionnel (mais encouragé) pour `owner`. Lib : `totp-rs`. Codes de récupération générés une fois, hashés en DB.
- **Rotation** des identifiants de session à chaque élévation de privilèges (après 2FA validé).
- **Rate limiting** : `tower-governor` + Redis. Limites plus strictes sur `/auth/*` (ex. 5 tentatives / 15 min / IP + lockout progressif par compte).

### 3.2 Transport & headers
- **TLS obligatoire** en prod (reverse proxy **Caddy** ou **Traefik** avec Let's Encrypt).
- Headers **toujours** présents :
  - `Strict-Transport-Security: max-age=63072000; includeSubDomains; preload`
  - `Content-Security-Policy` strict (pas de `unsafe-inline`, nonces pour les scripts)
  - `X-Content-Type-Options: nosniff`
  - `X-Frame-Options: DENY`
  - `Referrer-Policy: strict-origin-when-cross-origin`
  - `Permissions-Policy` restrictive (caméra, micro, géoloc désactivés sauf si besoin)
- **CORS** : liste blanche explicite d'origines. Jamais `*` avec credentials.

### 3.3 Données & logs
- **Chiffrement au repos** des champs sensibles (tokens OAuth OTAs, secrets Stripe côté app, PII sensibles) via `ring` / `aes-gcm` avec clé rotatable.
- **Audit log immuable** : table `audit_events` append-only (triggers DB empêchant UPDATE/DELETE), hash chaîné (`prev_hash` → `hash` style merkle-lite) pour détecter toute altération. Événements : login/logout, changement rôle, export données, action admin, paiement.
- **Pas de PII dans les logs applicatifs** : masking systématique (emails → `a***@b.com`, IBAN/cartes → never). Filtre `tracing` global.
- **Secrets** : jamais en clair dans le repo. `.env` gitignored, **Docker secrets** ou **SOPS** (+ age) en prod. `git-secrets` / `gitleaks` en CI.

### 3.4 Validation & injections
- **Validation d'entrée** sur *toutes* les routes (extractors Axum typés + `validator` crate ou Zod côté front). Rejeter avant d'atteindre la logique métier.
- SQL : **toujours** requêtes paramétrées SQLx. Jamais de string-concat.
- **Désérialisation stricte** : `#[serde(deny_unknown_fields)]` sur tous les DTOs d'entrée.

### 3.5 PCI-DSS & paiements
- **Jamais** stocker PAN, CVV, données magnétiques. Toute saisie carte via **Stripe Elements** / **Mollie Components** (tokens côté provider).
- Webhooks signés vérifiés (constant-time compare).

### 3.6 RGPD
- Endpoints d'**export** (JSON) et de **suppression** des données utilisateur.
- **Minimisation** : ne stocker que le strict nécessaire. Durées de rétention documentées par table.
- **Registre des traitements** versionné dans `/docs/rgpd/`.
- **DPIA** (analyse d'impact) consignée en ADR.

---

## 4. Tests — exigences

Aucune feature ne merge sans tests. Pyramide :

1. **Unitaires Rust** (`cargo test`) : logique pure, conversions, validators, erreurs.
2. **Intégration Rust** : tests contre une **vraie** Postgres de test (`testcontainers-rs` ou Postgres éphémère via docker-compose), **jamais** de mock de DB.
3. **E2E API** : **hurl** (`.hurl` versionnés) ou `reqwest` contre l'API dockerisée complète.
4. **Contract tests** : **schemathesis** sur l'OpenAPI, lancé en CI (fuzzing du contrat).
5. **Front unit/composants** : **Vitest** + **@vue/test-utils**.
6. **Front E2E** : **Playwright** (scénarios critiques : signup, login+2FA, création bien, sync iCal, paiement).

Couverture visée : **80 %** backend sur les modules métier, focus qualité > quantité.

---

## 5. Observabilité & logs

- Format logs : **JSON structuré** en prod (champs : `ts`, `level`, `target`, `span`, `trace_id`, `user_id` si applicable, `org_id`, `msg`, `fields...`).
- **`trace_id`** propagé via header `traceparent` (W3C) — corrélation front ↔ API ↔ workers.
- **Métriques** : exporter Prometheus (`axum-prometheus`) — latences, erreurs, saturation.
- **Dashboards Grafana** versionnés dans `/infra/grafana/dashboards/`.
- **Alertes** minimales dès le début : taux d'erreur 5xx, latence p95, erreurs d'auth anormales, échecs de webhook.

---

## 6. Intégrations externes

- **Calendriers OTAs** : sync **iCal** (Airbnb, Booking.com, VRBO). Worker dédié (queue RabbitMQ/NATS), parsing `icalendar`, détection conflits.
- **Paiements** : **Stripe** (défaut) ou **Mollie**. Toujours via webhooks signés. Idempotency-keys sur toutes les opérations.
- **Email transactionnel** : **Resend** ou **Postmark** (ou SMTP fallback). Templates versionnés (MJML → HTML).
- **Stockage fichiers** : **MinIO** (S3-compatible, self-host). URLs signées à durée limitée. Jamais d'URL publique.

---

## 7. Conventions de code

### 7.1 Rust
- `cargo fmt` et `cargo clippy --all-targets -- -D warnings` passent, toujours.
- Modules organisés par **domaine** (`bookings/`, `properties/`, `guests/`, `auth/`, `billing/`, `sync/`), pas par couche technique.
- Couche interne par domaine : `domain.rs` (types + règles), `repo.rs` (SQLx), `service.rs` (orchestration), `routes.rs` (handlers Axum), `dto.rs` (DTOs + utoipa schemas).
- Fonctions publiques d'un module = **API contract** : `///` rustdoc obligatoire.
- Pas de `unwrap()` / `expect()` en dehors des tests et du bootstrap. Toujours `?` + erreur typée.
- Async-partout, pas de blocking dans les handlers.

### 7.2 TypeScript / Vue
- Composants `<script setup lang="ts">` uniquement.
- Props typées via `defineProps<Props>()`.
- Pas de `any`. Pas de `as` hors cast justifié.
- Composables dans `composables/useXxx.ts`, un seul responsabilité par composable.
- Stores Pinia *fins* (pas de logique métier lourde — déléguer au backend).
- ESLint strict (`@typescript-eslint/strict-type-checked`), Prettier, `eslint-plugin-vue` recommended.

### 7.3 Git
- Commits : `type(scope): description courte en anglais` (`feat(bookings): add iCal conflict detection`).
- PRs petites, 1 intention = 1 PR. Review avec `security-review` skill avant merge sur PRs touchant `auth/`, `billing/`, `infra/`.

---

## 8. Documentation

- `README.md` : présentation, prérequis (Rust, Bun, Docker), setup dev en 5 min max, commandes usuelles.
- `docs/architecture.md` : diagrammes (C4 niveau 1-2), choix techniques **justifiés**.
- `docs/adr/NNNN-titre.md` : une ADR par décision structurante (format MADR).
- **rustdoc** obligatoire sur toute API publique backend.
- Doc API HTTP exposée via **Scalar** sur `/docs` (prod : derrière auth admin).

---

## 9. Règles de comportement pour Claude

Quand Claude travaille sur ce projet, il doit **systématiquement** :

1. **Proposer un plan avant de coder** toute tâche non-triviale (> 1 fichier ou > 30 lignes). Attendre validation.
2. **Écrire les tests en même temps que le code** — jamais de feature sans test correspondant.
3. **Justifier les choix sécurité** avec référence (OWASP ASVS §x.y, RFC nnnn, doc crate).
4. **Privilégier la simplicité** : YAGNI strict, pas d'abstraction prématurée, pas de "clever code". Lisible > astucieux.
5. **Ne jamais introduire** : axios, dépendance non-OSS, outil propriétaire, librairie non maintenue (< 6 mois sans commit = suspect).
6. **Ne jamais** supprimer / modifier une migration déjà mergée — toujours créer une nouvelle migration.
7. **Demander** avant d'ajouter une dépendance lourde (> 1 Mo compilé ou > 50 deps transitives).
8. **Respecter la langue** : code/identifiants/commits/rustdoc/README → **EN**. Messages utilisateur (i18n `fr.json`, erreurs HTTP `detail` côté front) → **FR** par défaut.
9. **Toujours lancer** `cargo fmt`, `clippy -D warnings`, `eslint`, `prettier` avant de déclarer terminé.
10. **Vérifier** qu'aucun secret n'est commit (scan mental + `gitleaks` si dispo).
11. **Signaler** proactivement toute dette technique ou faille potentielle repérée en passant, même hors scope.
12. **Refuser poliment** un raccourci qui viole §3 (sécurité) et proposer l'alternative correcte.

---

## 10. Anti-patterns interdits

- ❌ axios, fetch nu dans les composants, `any` en TS
- ❌ JWT stateless pour l'auth principale (OK pour webhooks internes courte durée)
- ❌ ORM "magique" (Diesel/SeaORM) — on reste sur SQLx brut
- ❌ `unwrap()` hors tests
- ❌ Logs avec PII en clair
- ❌ `eval`, `innerHTML`, `v-html` sur contenu non-sanitisé
- ❌ Migrations réversibles complexes — préférer forward-only + backups
- ❌ Feature flags stockés en code — utiliser DB/config si besoin
- ❌ Mocks de DB dans les tests d'intégration
- ❌ `.env` commité, secrets en dur, clefs dans le code
- ❌ Dépendances abandonnées ou GPL (licences à surveiller via `cargo deny`)

---

## 11. Checklist "prêt à merger"

- [ ] `cargo fmt` + `clippy -D warnings` OK
- [ ] `cargo test` (unit + integration) vert
- [ ] Nouveaux endpoints → OpenAPI à jour + test hurl + test schemathesis
- [ ] Nouveaux champs sensibles → chiffrés + masqués dans les logs
- [ ] Front : `bun lint` + `bun test` + `bun run build` OK
- [ ] Migration SQL forward-only + testée sur une base de dev reset
- [ ] ADR créée si décision structurante
- [ ] README / architecture.md mis à jour si impact
- [ ] Aucun secret / `.env` / clé dans le diff
- [ ] Commit message Conventional Commits

---

*Dernière révision : 2026-04-18. Toute modification de ce fichier passe par une ADR.*
