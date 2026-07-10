# Covername — Design System

## Brand Identity

**Concept**: A spy/intelligence agency aesthetic meets modern macOS app design. Clean, spacious, and professional — but with the distinctive visual language of classified documents and government redaction.

**Visual metaphor**: Declassified documents. Black redaction bars. Cover identities. The tool gives your documents a "cover name" before they go out into the world.

**Mascot/mark**: Consider a stylized redaction bar or a "CLASSIFIED" stamp as the logo mark. The chameleon concept may appear in marketing but the in-app identity leans into the spy/redaction theme.

---

## Design Tokens

All colors, spacing, and typography are defined as CSS custom properties (tokens). Changing the theme means changing token values — no component code changes required.

### Color Palette

```css
:root {
  /* Primary — Teal (trust, security, modern) */
  --color-primary-50: #f0fdfa;
  --color-primary-100: #ccfbf1;
  --color-primary-200: #99f6e4;
  --color-primary-300: #5eead4;
  --color-primary-400: #2dd4bf;
  --color-primary-500: #14b8a6;  /* Main brand color */
  --color-primary-600: #0d9488;
  --color-primary-700: #0f766e;
  --color-primary-800: #115e59;
  --color-primary-900: #134e4a;

  /* Neutral — Slate (clean, macOS-native feel) */
  --color-neutral-50: #f8fafc;
  --color-neutral-100: #f1f5f9;
  --color-neutral-200: #e2e8f0;
  --color-neutral-300: #cbd5e1;
  --color-neutral-400: #94a3b8;
  --color-neutral-500: #64748b;
  --color-neutral-600: #475569;
  --color-neutral-700: #334155;
  --color-neutral-800: #1e293b;
  --color-neutral-900: #0f172a;

  /* Semantic */
  --color-success: #10b981;
  --color-warning: #f59e0b;
  --color-error: #ef4444;
  --color-info: var(--color-primary-500);

  /* Redaction — The signature visual */
  --color-redaction: #0f0f0f;          /* Near-black bar */
  --color-redaction-hover: #1a1a1a;    /* Slightly lighter on hover */
  --color-redaction-text: #ffffff;      /* White text on redaction bar (replacement) */
  --color-highlight-pending: #fef3c7;  /* Amber highlight for unreviewed PII */
  --color-highlight-accepted: #d1fae5; /* Green highlight for accepted replacements */
  --color-highlight-rejected: #fecaca; /* Red highlight for rejected */

  /* Surface (light mode) */
  --color-bg: #ffffff;
  --color-bg-secondary: var(--color-neutral-50);
  --color-bg-tertiary: var(--color-neutral-100);
  --color-border: var(--color-neutral-200);
  --color-text: var(--color-neutral-900);
  --color-text-secondary: var(--color-neutral-500);
  --color-text-muted: var(--color-neutral-400);
}

/* Dark mode — follows system preference */
@media (prefers-color-scheme: dark) {
  :root {
    --color-bg: var(--color-neutral-900);
    --color-bg-secondary: var(--color-neutral-800);
    --color-bg-tertiary: var(--color-neutral-700);
    --color-border: var(--color-neutral-700);
    --color-text: var(--color-neutral-50);
    --color-text-secondary: var(--color-neutral-400);
    --color-text-muted: var(--color-neutral-500);
    --color-redaction: #1a1a1a;
    --color-redaction-text: #f8fafc;
  }
}
```

### Typography

Native macOS system font stack. No custom fonts to load.

```css
:root {
  --font-sans: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", system-ui, sans-serif;
  --font-mono: "SF Mono", ui-monospace, "Cascadia Code", "Fira Code", monospace;

  /* Scale — spacious, generous line heights */
  --text-xs: 0.75rem;     /* 12px — labels, captions */
  --text-sm: 0.8125rem;   /* 13px — secondary text, metadata */
  --text-base: 0.875rem;  /* 14px — body text (macOS standard) */
  --text-lg: 1rem;        /* 16px — section headers */
  --text-xl: 1.25rem;     /* 20px — page titles */
  --text-2xl: 1.5rem;     /* 24px — hero/splash */

  --leading-tight: 1.3;
  --leading-normal: 1.5;
  --leading-relaxed: 1.7;

  --weight-normal: 400;
  --weight-medium: 500;
  --weight-semibold: 600;
  --weight-bold: 700;
}
```

### Spacing

8px base grid. Spacious layout.

```css
:root {
  --space-1: 0.25rem;   /* 4px */
  --space-2: 0.5rem;    /* 8px */
  --space-3: 0.75rem;   /* 12px */
  --space-4: 1rem;      /* 16px */
  --space-5: 1.25rem;   /* 20px */
  --space-6: 1.5rem;    /* 24px */
  --space-8: 2rem;      /* 32px */
  --space-10: 2.5rem;   /* 40px */
  --space-12: 3rem;     /* 48px */
  --space-16: 4rem;     /* 64px */
}
```

### Border Radius

Rounded, friendly, modern.

```css
:root {
  --radius-sm: 6px;
  --radius-md: 8px;
  --radius-lg: 12px;
  --radius-xl: 16px;
  --radius-full: 9999px;  /* Pills, avatars */
}
```

### Shadows

Subtle, macOS-native depth.

```css
:root {
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.05);
  --shadow-md: 0 4px 6px -1px rgba(0, 0, 0, 0.07), 0 2px 4px -2px rgba(0, 0, 0, 0.05);
  --shadow-lg: 0 10px 15px -3px rgba(0, 0, 0, 0.08), 0 4px 6px -4px rgba(0, 0, 0, 0.03);
  --shadow-panel: 0 0 0 1px var(--color-border), 0 4px 12px rgba(0, 0, 0, 0.06);
}
```

---

## Component Patterns

### Redaction Bar (signature element)

The most distinctive UI element. Used to show detected PII and its replacement.

```
┌──────────────────────────────────────────────────────┐
│ "Jason Smith"  →  ████████████████  "John Adams"     │
│                   (thick black bar)                   │
└──────────────────────────────────────────────────────┘
```

- **Pending review**: Amber/yellow background highlight on the original text, no bar yet
- **Accepted**: Thick black bar over original, white replacement text shown below or beside
- **Rejected**: Dimmed text, strikethrough, left in place

```css
.redaction-bar {
  background: var(--color-redaction);
  color: var(--color-redaction-text);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  display: inline-block;
  min-width: 4ch;
  /* Slight height to look like a thick marker stroke */
  line-height: 1.6;
}
```

### Panels & Cards

macOS-native panel feel. Subtle borders, slight shadows, generous padding.

```css
.panel {
  background: var(--color-bg);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-panel);
  padding: var(--space-6);
}
```

### Buttons

```css
.btn-primary {
  background: var(--color-primary-600);
  color: white;
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  font-weight: var(--weight-medium);
  font-size: var(--text-sm);
}

.btn-secondary {
  background: transparent;
  color: var(--color-text);
  border: 1px solid var(--color-border);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  font-weight: var(--weight-medium);
  font-size: var(--text-sm);
}

.btn-accept {
  background: var(--color-success);
  color: white;
}

.btn-reject {
  background: var(--color-error);
  color: white;
}
```

---

## Layout

### Main App Structure

```
┌─────────────────────────────────────────────────────────────────┐
│  ┌─── Toolbar ──────────────────────────────────────────────┐   │
│  │  [📂 Open]  [Covername v0.1]           [⚙ Settings]      │   │
│  └───────────────────────────────────────────────────────────┘   │
│                                                                   │
│  ┌─── Document Viewer (2/3 width) ───┐  ┌── Sidebar (1/3) ──┐  │
│  │                                    │  │                    │  │
│  │  Page content with highlighted     │  │  Detections List   │  │
│  │  PII shown as redaction bars       │  │                    │  │
│  │                                    │  │  [1] ██████████    │  │
│  │  "Statement for:                   │  │      Jason Smith   │  │
│  │   ████████████ (John Adams)        │  │      → John Adams  │  │
│  │   Account: ██████████████          │  │      [✓] [✗] [✎]  │  │
│  │   SSN: ███████████"                │  │                    │  │
│  │                                    │  │  [2] ██████████    │  │
│  │                                    │  │      4521-8834...  │  │
│  │                                    │  │      → 9999-0000.. │  │
│  │                                    │  │      [✓] [✗] [✎]  │  │
│  │                                    │  │                    │  │
│  │                                    │  │  ─────────────     │  │
│  │                                    │  │  [Accept All]      │  │
│  │                                    │  │  [Generate Output] │  │
│  └────────────────────────────────────┘  └────────────────────┘  │
│                                                                   │
│  ┌─── Status Bar ────────────────────────────────────────────┐   │
│  │  4 detections · 2 accepted · sample.pdf                   │   │
│  └───────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Spacing Guidelines

- **Window padding**: `--space-6` (24px) on all sides
- **Panel gaps**: `--space-4` (16px) between major sections
- **Content padding inside panels**: `--space-6` (24px)
- **List item spacing**: `--space-3` (12px) between items
- **Button groups**: `--space-2` (8px) gap

---

## Interaction Patterns

### File Drop Zone

When no file is open, show a drop zone:
```
┌─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┐
│                                             │
│     Drop files here, or click to browse     │
│                                             │
│     📄 .txt  📋 .pdf  📊 .xlsx  🖼 images  │
│                                             │
└─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┘
```
- Dashed border (var(--color-border))
- On drag hover: border becomes solid teal, slight background tint

### Review Actions

Each detection in the sidebar has:
- **Accept** (✓) — green, applies the replacement
- **Reject** (✗) — red, leaves original
- **Edit** (✎) — opens inline text field to change replacement

Keyboard shortcuts: `y` accept, `n` reject, `e` edit, `a` accept all, `↑/↓` navigate

### Transitions

- Panel open/close: 200ms ease-out
- Highlight fade: 150ms
- Button hover: 100ms

---

## Iconography

Use system SF Symbols where possible (macOS native). Fallback to Lucide icons (open-source, consistent with rounded style).

Key icons:
- 📂 File open
- 🔍 Scan
- ✓ Accept
- ✗ Reject
- ✎ Edit
- ⚙ Settings
- 📥 Export
- 🗑 Remove

---

## Accessibility

- All interactive elements must be keyboard-navigable
- Minimum contrast ratio: 4.5:1 (WCAG AA)
- Focus rings: 2px solid var(--color-primary-500), 2px offset
- Redaction bars must have aria-label describing the original and replacement
- Screen reader: "Detected Jason Smith, replaced with John Adams, accepted"

---

## Future: Internationalization (i18n)

All user-facing strings should be externalized for future localization:
- Use a string key system (e.g., `t('detection.accept')`)
- Support RTL layouts
- Format dates/numbers per locale
- Initial release: English only, but architecture supports adding languages without code changes
