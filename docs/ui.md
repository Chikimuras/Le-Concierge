# UI conventions

Cadrage du design system côté `apps/web`. Objectif : cohérence visuelle
sans outillage lourd (pas de Storybook tant qu'on reste sous ~15
primitives). Cette doc fait foi — une nouvelle vue qui viole une de ces
règles se fait refuser en review.

---

## Stack

- **Tailwind** avec la config shadcn-vue d'origine
  (`apps/web/tailwind.config.js`). `darkMode: 'class'` ; `useTheme`
  gère la bascule.
- **shadcn-vue** comme source unique des primitives. Les composants
  sont *copiés* dans `apps/web/src/components/ui/` — on ne dépend
  d'aucun package shadcn runtime.
- **`cn()`** (dans `@/lib/utils`) mélange classes conditionnelles +
  `tailwind-merge`. Tous les composants l'utilisent pour fusionner les
  classes par défaut avec la prop `class`.
- **Icônes**: `lucide-vue-next` exclusivement. Pas d'autre pack.

---

## Tokens

Les couleurs et les radii sont définis en variables CSS (HSL) dans
`src/assets/main.css` et exposés via Tailwind sous des noms sémantiques.
**Ne jamais référencer une couleur brute** (`#fff`, `bg-gray-500`,
`hsl(...)`, valeur Tailwind arbitraire `bg-[#fff]`) dans les vues ou
composants.

| Rôle            | Classe Tailwind             | Quand                                                |
| --------------- | --------------------------- | ---------------------------------------------------- |
| Fond d'écran    | `bg-background`             | `<body>` / `<main>` racine                           |
| Texte principal | `text-foreground`           | Par défaut, pas besoin de l'écrire                   |
| Texte discret   | `text-muted-foreground`     | Sous-titres, légendes, métadonnées                   |
| Carte / panneau | `bg-card text-card-foreground` | Boîtes délimitées (formulaires, widgets, sections) |
| CTA primaire    | `bg-primary text-primary-foreground` | Bouton principal, liens d'accentuation         |
| Action dangereuse | `bg-destructive text-destructive-foreground` | Suppression, désactivation                   |
| Accent / hover  | `bg-accent text-accent-foreground` | Survol, états sélectionnés                      |
| Fond neutre     | `bg-muted`                  | Bandeau d'info, placeholder                          |
| Trait           | `border-border`             | Séparateurs, cadres                                  |
| Focus ring      | `ring-ring`                 | État `:focus-visible` (géré par les primitives)      |

Radii via `rounded-sm|md|lg` (respectivement `--radius - 4px`,
`--radius - 2px`, `--radius`). **Pas de `rounded-[Xpx]`** arbitraire.

Espacement : garder la grille Tailwind native (`gap-2`, `p-4`, `py-12`).
Pas de valeurs arbitraires en `[Xpx]` sauf si elles sortent volontairement
du rythme (ex. grille CSS `grid-cols-[auto_1fr]`).

---

## Primitives existantes

Toutes sous `apps/web/src/components/ui/`. Importer via l'alias `@` :

```vue
import Button from '@/components/ui/Button.vue'
```

| Composant | Props principales                                            | Usage                               |
| --------- | ------------------------------------------------------------ | ----------------------------------- |
| `Button`  | `variant` (default / destructive / outline / secondary / ghost / link), `size` (default / sm / lg / icon), `type`, `loading`, `disabled`, `class` | Action unique. `type="submit"` dans un formulaire. `loading` affiche un spinner et désactive. |
| `Card`    | `class`                                                      | Conteneur de section, par défaut `bg-card` + `border` + `shadow-sm`. |
| `FormField` | `label`, `hint`, `errorMessage`, `required`, slot `default` exposant `{ id, describedby, invalid }` | Un champ accessible. Toujours utilisé autour d'un `Input`. |
| `Input`   | `modelValue` (v-model), `invalid`, `class`                   | Input `<input>` stylé. `v-bind` des `*Attrs` de vee-validate. |
| `Label`   | `class`                                                      | `<label>` stylé. `FormField` s'en sert en interne. |

---

## Patterns obligatoires

### Formulaires

Toujours `FormField` + `Input` via le slot (jamais un `<input>` nu, jamais
un `<label>` nu) :

```vue
<FormField :label="t('auth.fields.email')" :error-message="errors.email" required>
  <template #default="{ id, describedby, invalid }">
    <Input
      :id="id"
      v-model="email"
      v-bind="emailAttrs"
      type="email"
      autocomplete="email"
      required
      :aria-describedby="describedby"
      :invalid="invalid"
    />
  </template>
</FormField>
```

Validation via **vee-validate + Zod** (schémas dans
`@/lib/api-contracts.ts`), jamais de `required` inline isolé.

### Sections

Une section d'UI visuellement délimitée est une `Card`. Pas de
`<div class="border rounded-lg bg-white ...">` reconstitué à la main.

### Actions

Un bouton = `Button`. Jamais `<button class="bg-primary ...">`. Pour un
lien qui ressemble à un bouton : `RouterLink` en mode `custom` + un
`<Button @click="navigate">` à l'intérieur (évite les `<button>` imbriqués
dans un `<a>`).

```vue
<RouterLink :to="{ name: 'settings' }" v-slot="{ navigate }" custom>
  <Button variant="ghost" @click="navigate">Paramètres</Button>
</RouterLink>
```

### États de chargement & erreur

- Les mutations TanStack Query exposent `isPending` / `isError` — à
  brancher sur `Button :loading` et sur un `<p role="alert">`.
- Erreurs serveur : passer par `readProblemDetails(err)` (de
  `@/lib/api-client`) et mapper `problem.kind` sur une clé i18n. Ne
  jamais afficher `problem.detail` brut à l'utilisateur.

### Feedback utilisateur

Toute erreur destinée à l'utilisateur passe par **`vue-i18n`** (`fr`
par défaut). Les messages d'alerte critiques portent
`role="alert"` ; les champs invalides portent `aria-invalid`
(géré par `Input` si on passe `invalid`).

---

## Accessibilité

Check-list à appliquer sur chaque nouveau formulaire / vue :

- [ ] Chaque `<input>` a un `<label>` lié (via `FormField` qui fait ça).
- [ ] `autocomplete` adéquat (`email`, `current-password`,
  `new-password`, `one-time-code`, `organization`, `off`).
- [ ] Code TOTP : `inputmode="numeric"`, `autocomplete="one-time-code"`,
  `maxlength="6"`.
- [ ] Messages d'erreur affichés avec `role="alert"` et référencés par
  `aria-describedby` sur le champ.
- [ ] Icônes purement décoratives : `aria-hidden="true"`.
- [ ] Éléments interactifs atteignables au clavier (aucun
  `@click` sur une `<div>` — utiliser `<button>` / `Button`).
- [ ] `alt` non vide pour chaque `<img>` porteuse de sens (ex. QR code).

---

## Ajouter une primitive

Quand une vue a besoin d'un composant qu'on n'a pas encore (select,
dialog, tabs, toast, table, tooltip, popover, switch, …), **installer
la version shadcn-vue** plutôt que hand-roller :

```bash
cd apps/web
bunx shadcn-vue@latest add <name>
```

La commande copie le composant sous `src/components/ui/<Name>.vue` avec
les bons tokens. Vérifier qu'aucune couleur brute / radius arbitraire ne
s'est glissé dans la copie, puis mettre à jour la table "Primitives
existantes" ci-dessus.

Avant de coder une primitive à la main, **toujours vérifier le catalogue
shadcn-vue** en premier.

---

## Anti-patterns interdits

- `v-html` sur contenu serveur ou utilisateur (CLAUDE.md §10).
- Couleurs / radius arbitraires (`bg-[#xxx]`, `rounded-[5px]`).
- `<button class="bg-primary ...">` — passer par `Button`.
- `<input>` / `<label>` nus — passer par `FormField` + `Input`.
- `<a>` englobant `<Button>` (HTML invalide) — `RouterLink custom`.
- `t('...')` sur une clé i18n array — utiliser `i18n.tm(...)` et typer
  via `computed<string[]>`.
- Redondance de tokens : si tu écris `text-sm text-muted-foreground`
  trois fois dans une vue, c'est peut-être un composant utilitaire
  (même si on reste sous le seuil "Rule of three" tant qu'on a < 3
  duplications).

---

## Quand réévaluer

Cette doc reste légère tant qu'on a < 15 primitives et un seul
développeur. Passé ce seuil, ouvrir une ADR pour décider :
- Storybook ou Histoire pour les snapshots visuels + régressions.
- Design tokens formalisés (fichier CSS indépendant, synchronisation
  avec Figma si pertinent).
- Tests de contraste automatisés (axe-core en CI).

Référence : CLAUDE.md §2.2 (stack front), §7.2 (conventions TS/Vue),
§10 (anti-patterns).
