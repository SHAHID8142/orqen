import { NAV_ITEMS, type ViewId } from "./views";

/**
 * Primary workspace navigation.
 *
 * Buttons (keyboard-activatable) with `aria-current="page"` marking the
 * active surface, per the WAI-ARIA APG pattern for SPA view switching.
 */
function SidebarNav({
  activeView,
  onNavigate,
}: {
  activeView: ViewId;
  onNavigate: (view: ViewId) => void;
}) {
  return (
    <nav className="sidebar-nav" aria-label="Workspace sections">
      <ul>
        {NAV_ITEMS.map((item) => (
          <li key={item.id}>
            <button
              type="button"
              className="nav-item"
              aria-current={activeView === item.id ? "page" : undefined}
              onClick={() => onNavigate(item.id)}
            >
              {item.label}
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}

export default SidebarNav;
