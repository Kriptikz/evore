"use client";

interface MetricCardProps {
  title: string;
  value: string | number;
  subtitle?: string;
  icon?: React.ReactNode;
  trend?: {
    direction: "up" | "down" | "neutral";
    value: string;
  };
  color?: "blue" | "green" | "amber" | "red" | "slate";
}

const colorClasses = {
  blue: "bg-blue-500/10 border-blue-500/30 text-blue-400",
  green: "bg-green-500/10 border-green-500/30 text-green-400",
  amber: "bg-amber-500/10 border-amber-500/30 text-amber-400",
  red: "bg-red-500/10 border-red-500/30 text-red-400",
  slate: "bg-slate-700/50 border-slate-600 text-slate-300",
};

const iconBgClasses = {
  blue: "bg-blue-500/20",
  green: "bg-green-500/20",
  amber: "bg-amber-500/20",
  red: "bg-red-500/20",
  slate: "bg-slate-600/50",
};

export function MetricCard({
  title,
  value,
  subtitle,
  icon,
  trend,
  color = "slate",
}: MetricCardProps) {
  return (
    <div className={`rounded-xl border p-5 ${colorClasses[color]}`}>
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <p className="text-sm font-medium text-slate-400 mb-1">{title}</p>
          <p className="text-2xl font-bold text-white">{value}</p>
          {subtitle && (
            <p className="text-xs text-slate-500 mt-1">{subtitle}</p>
          )}
          {trend && (
            <div className="flex items-center gap-1 mt-2">
              {trend.direction === "up" && (
                <svg className="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 10l7-7m0 0l7 7m-7-7v18" />
                </svg>
              )}
              {trend.direction === "down" && (
                <svg className="w-4 h-4 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                </svg>
              )}
              <span className={`text-xs ${
                trend.direction === "up" ? "text-green-400" :
                trend.direction === "down" ? "text-red-400" :
                "text-slate-400"
              }`}>
                {trend.value}
              </span>
            </div>
          )}
        </div>
        {icon && (
          <div className={`w-10 h-10 rounded-lg flex items-center justify-center ${iconBgClasses[color]}`}>
            {icon}
          </div>
        )}
      </div>
    </div>
  );
}

