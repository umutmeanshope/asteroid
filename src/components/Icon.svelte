<script lang="ts" module>
  // SF Symbols-inspired line icons on a 24px grid. Each entry is SVG markup
  // drawn with `currentColor` strokes; dots use filled circles.
  const dot = (cx: number, cy: number, r = 1.25) => `<circle cx="${cx}" cy="${cy}" r="${r}" fill="currentColor" stroke="none"/>`;
  const docOutline = `<path d="M14 3H7.5A2.5 2.5 0 0 0 5 5.5v13A2.5 2.5 0 0 0 7.5 21h9a2.5 2.5 0 0 0 2.5-2.5V8z"/><path d="M14 3v3.5A1.5 1.5 0 0 0 15.5 8H19"/>`;
  const cloud = `<path d="M7 18.5a4.5 4.5 0 0 1-.7-8.95A6 6 0 0 1 17.9 9a4.75 4.75 0 0 1-.4 9.5z"/>`;
  const lens = `<circle cx="10.5" cy="10.5" r="6.5"/><path d="m20 20-4.6-4.6"/>`;
  const folderShape = `<path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h3.2a2 2 0 0 1 1.5.68L11.5 7.2h7A2.5 2.5 0 0 1 21 9.7v7.8a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5z"/>`;

  export const ICONS = {
    sidebar: `<rect x="3" y="4.5" width="18" height="15" rx="3.5"/><path d="M9.5 4.5v15"/><path d="M5.6 8h1.6M5.6 10.8h1.6"/>`,
    "sidebar-right": `<rect x="3" y="4.5" width="18" height="15" rx="3.5"/><path d="M14.5 4.5v15"/><path d="M16.8 8h1.6M16.8 10.8h1.6"/>`,
    compose: `<path d="M11.5 4.5H7.5a3 3 0 0 0-3 3v9a3 3 0 0 0 3 3h9a3 3 0 0 0 3-3v-4"/><path d="M18.1 3.9a1.9 1.9 0 0 1 2.7 2.7l-7.6 7.6-3.5.9.9-3.5z"/>`,
    folder: folderShape,
    "folder-plus": `${folderShape}<path d="M12 10.5v5.5M9.25 13.25h5.5"/>`,
    note: `${docOutline}<path d="M8.75 12.5h6.5M8.75 16h4.5"/>`,
    pdf: `${docOutline}<path d="M8.5 17v-4.5h1.4a1.4 1.4 0 0 1 0 2.8H8.5"/><path d="M13 12.5v4.5h.8a1.6 1.6 0 0 0 1.6-1.6v-1.3a1.6 1.6 0 0 0-1.6-1.6z"/>`,
    image: `<rect x="3" y="4.5" width="18" height="15" rx="3.5"/>${dot(8.5, 9.75, 1.6)}<path d="m20.5 15.5-4.3-4.3a1.5 1.5 0 0 0-2.1 0L5.5 19.5"/>`,
    file: docOutline,
    search: lens,
    table: `<rect x="3.5" y="4.5" width="17" height="15" rx="3"/><path d="M3.5 9.5h17M3.5 14.5h17M9.5 9.5v10M15 9.5v10"/>`,
    "row-above": `<rect x="3.5" y="11" width="17" height="9" rx="2.5"/><path d="M3.5 15.5h17"/><path d="M12 2.5v5.5M9.25 5.25h5.5"/>`,
    "row-below": `<rect x="3.5" y="4" width="17" height="9" rx="2.5"/><path d="M3.5 8.5h17"/><path d="M12 16v5.5M9.25 18.75h5.5"/>`,
    "col-left": `<rect x="11" y="3.5" width="9" height="17" rx="2.5"/><path d="M15.5 3.5v17"/><path d="M2.5 12H8M5.25 9.25v5.5"/>`,
    "col-right": `<rect x="4" y="3.5" width="9" height="17" rx="2.5"/><path d="M8.5 3.5v17"/><path d="M16 12h5.5M18.75 9.25v5.5"/>`,
    clock: `<circle cx="12" cy="12" r="8.5"/><path d="M12 7.25V12l3.25 2"/>`,
    restore: `<path d="M4.5 12a7.5 7.5 0 1 0 2.2-5.3"/><path d="M4.25 4.75V8.5H8"/><path d="M12 8.25V12l2.75 1.75"/>`,
    git: `<circle cx="6.5" cy="5.5" r="2.25"/><circle cx="6.5" cy="18.5" r="2.25"/><circle cx="17.5" cy="7.5" r="2.25"/><path d="M6.5 7.75v8.5"/><path d="M17.5 9.75c0 4.25-4.5 4.1-9.4 7"/>`,
    lock: `<rect x="5" y="10.5" width="14" height="10" rx="2.75"/><path d="M8.25 10.5V8a3.75 3.75 0 0 1 7.5 0v2.5"/>`,
    scissors: `<circle cx="6.5" cy="17.5" r="2.75"/><circle cx="17.5" cy="17.5" r="2.75"/><path d="M8.4 15.4 17.5 3.5M15.6 15.4 6.5 3.5"/>`,
    copy: `<rect x="8.5" y="8.5" width="12" height="12" rx="2.75"/><path d="M15.5 8.5V6.25a2.75 2.75 0 0 0-2.75-2.75h-6.5A2.75 2.75 0 0 0 3.5 6.25v6.5a2.75 2.75 0 0 0 2.75 2.75H8.5"/>`,
    clipboard: `<path d="M8.5 4.5H7.25A2.25 2.25 0 0 0 5 6.75v12A2.25 2.25 0 0 0 7.25 21h9.5A2.25 2.25 0 0 0 19 18.75v-12a2.25 2.25 0 0 0-2.25-2.25H15.5"/><rect x="8.5" y="3" width="7" height="3.5" rx="1.25"/>`,
    bold: `<path d="M7.5 4.5h5.25a3.75 3.75 0 0 1 0 7.5H7.5zM7.5 12h6.25a3.75 3.75 0 0 1 0 7.5H7.5z"/>`,
    italic: `<path d="M10.5 4.5h7M6.5 19.5h7M14 4.5l-4 15"/>`,
    strikethrough: `<path d="M4.5 12h15"/><path d="M16.5 7.75c-.5-1.9-2.3-3.25-4.6-3.25-2.6 0-4.4 1.45-4.4 3.4 0 1.2.6 2.2 2 2.85"/><path d="M7.5 16.25c.5 1.9 2.4 3.25 4.75 3.25 2.6 0 4.5-1.45 4.5-3.5 0-.7-.2-1.3-.6-1.8"/>`,
    code: `<path d="m8.5 7-5 5 5 5M15.5 7l5 5-5 5"/>`,
    checklist: `<rect x="3.5" y="4.5" width="6" height="6" rx="1.75"/><path d="m4.6 16.6 1.7 1.7 3-3.3"/><path d="M13 7.5h7.5M13 16.5h7.5"/>`,
    "select-all": `<rect x="4" y="4" width="16" height="16" rx="3.5" stroke-dasharray="3.2 2.6"/>`,
    tag: `<path d="M3.5 4.9v6.3c0 .5.2 1 .6 1.4l7.8 7.8a2 2 0 0 0 2.8 0l5.7-5.7a2 2 0 0 0 0-2.8l-7.8-7.8a2 2 0 0 0-1.4-.6H4.9a1.4 1.4 0 0 0-1.4 1.4z"/>${dot(8, 8.5, 1.4)}`,
    hash: `<path d="M5 9h15M4 15h15M10.5 3.5 8.5 20.5M15.5 3.5l-2 17"/>`,
    gear: `<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68 1.65 1.65 0 0 0 10 3.17V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>`,
    "chevron-right": `<path d="m9.5 6 6 6-6 6"/>`,
    "chevron-left": `<path d="m14.5 6-6 6 6 6"/>`,
    "chevron-down": `<path d="m6 9.5 6 6 6-6"/>`,
    "chevron-up-down": `<path d="m8 9 4-4 4 4M8 15l4 4 4-4"/>`,
    xmark: `<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>`,
    plus: `<path d="M12 5v14M5 12h14"/>`,
    minus: `<path d="M5 12h14"/>`,
    ellipsis: `${dot(6, 12, 1.5)}${dot(12, 12, 1.5)}${dot(18, 12, 1.5)}`,
    book: `<path d="M12 6.5C10.6 5.4 8.6 4.8 6 4.8c-.9 0-1.8.1-2.5.3v13.6c.7-.2 1.6-.3 2.5-.3 2.6 0 4.6.6 6 1.7 1.4-1.1 3.4-1.7 6-1.7.9 0 1.8.1 2.5.3V5.1c-.7-.2-1.6-.3-2.5-.3-2.6 0-4.6.6-6 1.7z"/><path d="M12 6.5v13.6"/>`,
    pencil: `<path d="M16.6 3.9a2.1 2.1 0 0 1 3 3L8.2 18.3l-4.2 1.2 1.2-4.2z"/><path d="m14.5 6 3.5 3.5"/>`,
    outline: `${dot(5, 6.5)}${dot(8.5, 12)}${dot(8.5, 17.5)}<path d="M9 6.5h11M12.5 12H20M12.5 17.5H20"/>`,
    link: `<path d="M10 14a4.2 4.2 0 0 0 6 0l3-3a4.24 4.24 0 0 0-6-6l-1.2 1.2"/><path d="M14 10a4.2 4.2 0 0 0-6 0l-3 3a4.24 4.24 0 0 0 6 6l1.2-1.2"/>`,
    sync: `<path d="M19.5 10.5A7.6 7.6 0 0 0 5.6 7.6M4.5 4v4h4"/><path d="M4.5 13.5a7.6 7.6 0 0 0 13.9 2.9M19.5 20v-4h-4"/>`,
    cloud,
    "cloud-check": `${cloud}<path d="m9.25 13.25 2 2 3.75-3.75"/>`,
    "cloud-off": `${cloud}<path d="M3.5 3.5l17 17"/>`,
    "cloud-down": `${cloud}<path d="M12 10.5v5.5M9.5 13.5 12 16l2.5-2.5"/>`,
    warning: `<path d="M10.3 4.9a2 2 0 0 1 3.4 0l7.1 12.3a2 2 0 0 1-1.7 3H4.9a2 2 0 0 1-1.7-3z"/><path d="M12 10v4"/>${dot(12, 17, 1.1)}`,
    check: `<path d="m5 12.5 4.5 4.5L19 7.5"/>`,
    trash: `<path d="M4.5 7h15M10 11v6M14 11v6"/><path d="m6 7 .9 11.2a2 2 0 0 0 2 1.8h6.2a2 2 0 0 0 2-1.8L18 7"/><path d="M9 7V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V7"/>`,
    "arrow-left": `<path d="M19 12H5M11 6l-6 6 6 6"/>`,
    "arrow-right": `<path d="M5 12h14M13 6l6 6-6 6"/>`,
    "zoom-in": `${lens}<path d="M10.5 7.75v5.5M7.75 10.5h5.5"/>`,
    "zoom-out": `${lens}<path d="M7.75 10.5h5.5"/>`,
    command: `<path d="M9 6.5A2.5 2.5 0 1 0 6.5 9H9zm0 0v11m0-11h6m-6 11A2.5 2.5 0 1 1 6.5 15H9zm0 0h6m0-11A2.5 2.5 0 1 1 17.5 9H15zm0 0v11m0 0a2.5 2.5 0 1 0 2.5-2.5H15z"/>`,
    vault: `<rect x="3.5" y="4.5" width="17" height="4.5" rx="1.5"/><path d="M5 9v8.5A2.5 2.5 0 0 0 7.5 20h9a2.5 2.5 0 0 0 2.5-2.5V9"/><path d="M10 13h4"/>`,
    drive: `<path d="M8.6 3.8h6.8l6.1 10.5-3.4 5.9H5.9l-3.4-5.9z"/><path d="M8.6 3.8l6.2 10.5M15.4 3.8 9.2 14.3M2.5 14.3h19"/>`,
    "fit-width": `<path d="M4 12h16M7.5 8.5 4 12l3.5 3.5M16.5 8.5 20 12l-3.5 3.5"/>`,
    external: `<path d="M14 4h6v6M20 4l-8.5 8.5"/><path d="M18 14v3.5a2.5 2.5 0 0 1-2.5 2.5h-9A2.5 2.5 0 0 1 4 17.5v-9A2.5 2.5 0 0 1 6.5 6H10"/>`,
    reveal: `${folderShape}<path d="m10.5 11.5 3 2.5-3 2.5"/>`,
    sparkle: `<path d="M12 3.5c.4 4.3 2.2 6.1 6.5 6.5-4.3.4-6.1 2.2-6.5 6.5-.4-4.3-2.2-6.1-6.5-6.5 4.3-.4 6.1-2.2 6.5-6.5z"/>`,
    sun: `<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M4.6 4.6 6 6M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4 6 18M18 6l1.4-1.4"/>`,
    moon: `<path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/>`,
  } as const;

  export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
  let { name, size = 16, stroke = 1.7, class: klass = "" }: {
    name: IconName;
    size?: number;
    stroke?: number;
    class?: string;
  } = $props();
</script>

<svg
  class="icon {klass}"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {@html ICONS[name]}
</svg>

<style>
  .icon {
    flex: none;
    display: block;
  }
</style>
