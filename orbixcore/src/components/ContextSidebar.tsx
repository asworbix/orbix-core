import { useState, useMemo } from 'react';
import type { TabId, PanelType } from '../types';
import './ContextSidebar.css';

interface Props {
  activeTab: TabId;
  activePanel: PanelType;
  onToggle: (panel: PanelType) => void;
}

type NavItem = { id: PanelType; title: string; icon: React.ReactNode };
type NavGroup = { label: string; items: NavItem[]; defaultOpen: boolean };

const icon = (d: string, w = 16, h = 16) => (
  <svg width={w} height={h} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">{
    d.split('|').map((path, i) => {
      if (path.startsWith('C')) return <circle key={i} cx={path.split(',')[1]} cy={path.split(',')[2]} r={path.split(',')[3]} />;
      if (path.startsWith('R')) { const p = path.split(','); return <rect key={i} x={p[1]} y={p[2]} width={p[3]} height={p[4]} rx={p[5] || '0'} />; }
      if (path.startsWith('L')) { const p = path.split(','); return <line key={i} x1={p[1]} y1={p[2]} x2={p[3]} y2={p[4]} />; }
      return <path key={i} d={path} />;
    })
  }</svg>
);

/* ── Run - keepers only; legacy lives behind Cmd-K → All Panels ── */
const RUN_NAV: NavItem[] = [
  { id: 'agents', title: 'Agents', icon: icon('C,12,8,4|M5 20c0-4 3.1-7 7-7s7 3 7 7') },
  { id: 'my-ui', title: 'My UI', icon: icon('R,3,3,18,18,2|L,3,9,21,9|L,9,21,9,9') },
  { id: 'documents', title: 'Documents', icon: icon('M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z|M14 2v6h6') },
  { id: 'inbox', title: 'Inbox', icon: icon('M22 12h-6l-2 3h-4l-2-3H2|M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z') },
  { id: 'notifications', title: 'Notifications', icon: icon('M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9|M13.73 21a2 2 0 0 1-3.46 0') },
  { id: 'guide', title: 'How to', icon: icon('C,12,12,10|M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3|L,12,17,12.01,17') },

];

/* ── Build - grouped, keepers first ── */
const BUILD_GROUPS: NavGroup[] = [
  { label: 'Core', defaultOpen: true, items: [
    { id: 'agents', title: 'Agents', icon: icon('C,12,8,4|M5 20c0-4 3.1-7 7-7s7 3 7 7') },
      { id: 'connectors', title: 'Connectors', icon: icon('M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71|M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71') },
    { id: 'memory', title: 'Memory', icon: icon('M12 2L2 7l10 5 10-5-10-5z|M2 17l10 5 10-5|M2 12l10 5 10-5') },
    { id: 'my-ui', title: 'My UI', icon: icon('R,3,3,18,18,2|L,3,9,21,9|L,9,21,9,9') },
  ]},
  { label: 'More', defaultOpen: false, items: [
    { id: 'knowledge', title: 'Knowledge', icon: icon('M4 19.5A2.5 2.5 0 0 1 6.5 17H20|M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z') },
    { id: 'mcp', title: 'MCP', icon: icon('M4 17l6-6-6-6|L,12,19,20,19') },
    { id: 'tools', title: 'Tools', icon: icon('M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z') },
    { id: 'plugins', title: 'Plugins', icon: icon('R,3,3,7,7,1|R,14,3,7,7,1|R,3,14,7,7,1|R,14,14,7,7,1') },
    { id: 'browser', title: 'Browser', icon: icon('C,12,12,10|L,2,12,22,12|M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z') },
    { id: 'voice', title: 'Voice', icon: icon('M12 2a3 3 0 0 1 3 3v7a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3z|M19 10v2a7 7 0 0 1-14 0v-2') },
    { id: 'window-intel', title: 'Windows', icon: icon('R,2,3,20,14,2|C,12,10,3|L,8,21,16,21|L,12,17,12,21') },
    { id: 'search', title: 'Search', icon: icon('C,11,11,8|L,21,21,16.65,16.65') },
    { id: 'scheduler', title: 'Scheduler', icon: icon('C,12,12,10|M12 6v6l4 2') },
    { id: 'orchestrator', title: 'Chains', icon: icon('C,5,12,3|C,19,6,3|C,19,18,3|M8 12h4l3-6|M12 12l3 6') },
    { id: 'triggers', title: 'Triggers', icon: icon('M22 12h-4l-3 9L9 3l-3 9H2') },
  ]},
];

/* ── Watch - grouped, keepers first ── */


/* ── Code - flat ── */

const COMMON_NAV: NavItem[] = [
  { id: 'data-control', title: 'Data Control', icon: icon('M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z') },
];

/* ── Helpers ── */

function hasActiveItem(items: NavItem[], activePanel: PanelType): boolean {
  return items.some(item => item.id === activePanel);
}

/* ── Component ── */

export default function ContextSidebar({ activeTab, activePanel, onToggle }: Props) {
  const [expanded, setExpanded] = useState(false);

  // Determine if this tab uses groups
  const groups: NavGroup[] | null = activeTab === 'build' ? BUILD_GROUPS : null;

  const flatItems: NavItem[] | null = activeTab === 'run' ? RUN_NAV : null;

  // Track which groups are open - default open + any group containing active panel
  const [openGroups, setOpenGroups] = useState<Set<string>>(() => {
    const s = new Set<string>();
    if (groups) groups.forEach(g => { if (g.defaultOpen) s.add(g.label); });
    return s;
  });

  // Auto-open group containing active panel
  const effectiveOpen = useMemo(() => {
    const s = new Set(openGroups);
    if (groups) {
      // Always keep default-open groups open
      groups.forEach(g => { if (g.defaultOpen) s.add(g.label); });
      // Open group containing active panel
      if (activePanel) {
        groups.forEach(g => {
          if (g.items.some(item => item.id === activePanel)) s.add(g.label);
        });
      }
    }
    return s;
  }, [openGroups, groups, activePanel]);

  const toggleGroup = (label: string) => {
    setOpenGroups(prev => {
      const next = new Set(prev);
      if (next.has(label)) next.delete(label);
      else next.add(label);
      return next;
    });
  };

  const renderItem = (item: NavItem) => {
    return (
      <button
        key={item.id}
        className={`context-sidebar-btn ${activePanel === item.id ? 'active' : ''} `}
        onClick={() => onToggle(item.id)}
        title={expanded ? undefined : item.title}
      >
        <span className="sidebar-btn-icon">{item.icon}</span>
        {expanded && <span className="sidebar-btn-label">{item.title}</span>}
      </button>
    );
  };

  const renderGroup = (group: NavGroup) => {
    const isOpen = effectiveOpen.has(group.label);
    const hasActive = hasActiveItem(group.items, activePanel);
    const count = group.items.length;

    return (
      <div key={group.label} className="context-sidebar-group">
        {/* Group header */}
        <button
          className={`context-sidebar-group-header ${hasActive ? 'has-active' : ''}`}
          onClick={() => toggleGroup(group.label)}
          title={expanded ? undefined : `${group.label} (${count})`}
        >
          {expanded ? (
            <>
              <span className="group-header-label">{group.label}</span>
              <span className="group-header-count">{count}</span>
              <svg className={`group-header-chevron ${isOpen ? 'open' : ''}`} width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <polyline points="6 9 12 15 18 9" />
              </svg>
            </>
          ) : (
            <>
              {isOpen ? (
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <line x1="5" y1="12" x2="19" y2="12" />
                </svg>
              ) : (
                <span className="group-collapsed-count">+{count}</span>
              )}
            </>
          )}
        </button>

        {/* Group items */}
        {isOpen && group.items.map(renderItem)}
      </div>
    );
  };

  return (
    <div className={`context-sidebar ${expanded ? 'expanded' : ''}`}>
      {/* Expand/collapse toggle */}
      <button
        className="context-sidebar-toggle"
        onClick={() => setExpanded(!expanded)}
        title={expanded ? 'Collapse sidebar' : 'Expand sidebar'}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          {expanded
            ? <polyline points="15 18 9 12 15 6" />
            : <polyline points="9 18 15 12 9 6" />
          }
        </svg>
      </button>

      <div className="context-sidebar-items">
        {/* Flat nav (Run, Twin) */}
        {flatItems && flatItems.map(renderItem)}

        {/* Grouped nav (Build, Watch) */}
        {groups && groups.map(renderGroup)}
      </div>

      <div className="context-sidebar-spacer" />

      <div className="context-sidebar-items">
        {COMMON_NAV.map(renderItem)}
      </div>
    </div>
  );
}
