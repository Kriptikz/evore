/**
 * Recharts theme configuration matching the ORE Stats dark UI
 * 
 * Color palette matches the existing Tailwind classes used throughout the app.
 */

// Primary colors
export const colors = {
  // Primary accent (amber)
  primary: "#fbbf24",        // amber-400
  primaryDark: "#f59e0b",    // amber-500
  primaryLight: "#fcd34d",   // amber-300
  
  // Positive values (emerald/green)
  positive: "#34d399",       // emerald-400
  positiveDark: "#10b981",   // emerald-500
  
  // Negative values (red)
  negative: "#f87171",       // red-400
  negativeDark: "#ef4444",   // red-500
  
  // Secondary accents
  purple: "#c084fc",         // purple-400
  purpleDark: "#a855f7",     // purple-500
  blue: "#60a5fa",           // blue-400
  blueDark: "#3b82f6",       // blue-500
  cyan: "#22d3ee",           // cyan-400
  orange: "#fb923c",         // orange-400
  
  // Background colors
  background: "#0f172a",     // slate-900
  backgroundLight: "#1e293b", // slate-800
  backgroundDark: "#020617", // slate-950
  
  // Text colors
  text: "#cbd5e1",           // slate-300
  textMuted: "#64748b",      // slate-500
  textDark: "#94a3b8",       // slate-400
  
  // Grid/border colors
  grid: "#334155",           // slate-700
  gridLight: "#475569",      // slate-600
  border: "#1e293b",         // slate-800
};

// Chart-specific theme settings
export const chartTheme = {
  // Axis styling
  axis: {
    stroke: colors.grid,
    strokeWidth: 1,
    tick: {
      fill: colors.textMuted,
      fontSize: 11,
      fontFamily: "system-ui, -apple-system, sans-serif",
    },
    label: {
      fill: colors.textDark,
      fontSize: 12,
      fontWeight: 500,
    },
  },
  
  // Grid styling
  grid: {
    stroke: colors.grid,
    strokeDasharray: "3 3",
    strokeOpacity: 0.3,
  },
  
  // Tooltip styling
  tooltip: {
    backgroundColor: colors.backgroundLight,
    borderColor: colors.border,
    borderRadius: 8,
    padding: 12,
    textColor: colors.text,
    labelColor: colors.textMuted,
  },
  
  // Legend styling
  legend: {
    textColor: colors.textDark,
    fontSize: 12,
  },
  
  // Animation settings
  animation: {
    duration: 300,
    easing: "ease-out",
  },
};

// Gradient definitions for area charts
export const gradients = {
  primary: {
    id: "gradientPrimary",
    stops: [
      { offset: "0%", color: colors.primary, opacity: 0.4 },
      { offset: "100%", color: colors.primary, opacity: 0.05 },
    ],
  },
  positive: {
    id: "gradientPositive",
    stops: [
      { offset: "0%", color: colors.positive, opacity: 0.4 },
      { offset: "100%", color: colors.positive, opacity: 0.05 },
    ],
  },
  purple: {
    id: "gradientPurple",
    stops: [
      { offset: "0%", color: colors.purple, opacity: 0.4 },
      { offset: "100%", color: colors.purple, opacity: 0.05 },
    ],
  },
  blue: {
    id: "gradientBlue",
    stops: [
      { offset: "0%", color: colors.blue, opacity: 0.4 },
      { offset: "100%", color: colors.blue, opacity: 0.05 },
    ],
  },
};

// Series colors for multi-line/bar charts
export const seriesColors = [
  colors.primary,
  colors.positive,
  colors.purple,
  colors.blue,
  colors.cyan,
  colors.orange,
];

// Format helpers for chart display
export const formatters = {
  // Format lamports to SOL with appropriate decimals
  sol: (lamports: number): string => {
    const sol = lamports / 1e9;
    if (sol === 0) return "0";
    if (Math.abs(sol) >= 1000) return `${(sol / 1000).toFixed(1)}K`;
    if (Math.abs(sol) >= 1) return sol.toFixed(2);
    if (Math.abs(sol) >= 0.001) return sol.toFixed(4);
    return sol.toFixed(6);
  },
  
  // Format atomic ORE units to whole ORE
  ore: (atomic: number): string => {
    const ore = atomic / 1e11;
    if (ore === 0) return "0";
    if (Math.abs(ore) >= 1000000) return `${(ore / 1000000).toFixed(2)}M`;
    if (Math.abs(ore) >= 1000) return `${(ore / 1000).toFixed(1)}K`;
    if (Math.abs(ore) >= 1) return ore.toFixed(2);
    return ore.toFixed(4);
  },
  
  // Format large numbers with K/M suffix
  number: (value: number): string => {
    if (value === 0) return "0";
    if (Math.abs(value) >= 1000000) return `${(value / 1000000).toFixed(1)}M`;
    if (Math.abs(value) >= 1000) return `${(value / 1000).toFixed(1)}K`;
    return value.toLocaleString();
  },
  
  // Full precision number (no abbreviation)
  numberFull: (value: number): string => {
    return value.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    });
  },
  
  // Full precision ORE (no abbreviation, expects already-converted ORE value)
  oreFull: (ore: number): string => {
    return ore.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    });
  },
  
  // Full precision ORE from atomic units (no abbreviation)
  oreAtomicFull: (atomic: number): string => {
    return (atomic / 1e11).toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    });
  },
  
  // Format percentage
  percent: (value: number): string => {
    return `${(value * 100).toFixed(1)}%`;
  },
  
  // Format timestamp to readable date/time
  dateTime: (timestamp: number): string => {
    const date = new Date(timestamp * 1000);
    return date.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  },
  
  // Format timestamp to date only
  date: (timestamp: number): string => {
    const date = new Date(timestamp * 1000);
    return date.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
    });
  },
};

// Chart margin defaults
export const margins = {
  default: { top: 10, right: 10, bottom: 20, left: 40 },
  withLegend: { top: 10, right: 10, bottom: 40, left: 40 },
  compact: { top: 5, right: 5, bottom: 15, left: 30 },
};

