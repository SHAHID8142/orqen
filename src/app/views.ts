/**
 * Navigation model for the workspace shell.
 *
 * State-based view switching (D-013): five fixed surfaces, no router
 * dependency, no deep linking yet. If deep-linkable URLs become a
 * requirement, revisit this decision — do not grow it ad hoc.
 */

export type ViewId = "overview" | "projects" | "agents" | "workflows" | "settings";

export interface NavItem {
  id: ViewId;
  label: string;
}

/** Ordered navigation entries rendered in the sidebar. */
export const NAV_ITEMS: readonly NavItem[] = [
  { id: "overview", label: "Overview" },
  { id: "projects", label: "Projects" },
  { id: "agents", label: "Agents" },
  { id: "workflows", label: "Workflows" },
  { id: "settings", label: "Settings" },
];

/** Widths at or below this value switch the sidebar to overlay mode (CSS mirrors this). */
export const NARROW_WIDTH_QUERY = "(max-width: 900px)";

export function isDefaultView(view: string | null): view is ViewId {
  return NAV_ITEMS.some((item) => item.id === view);
}
