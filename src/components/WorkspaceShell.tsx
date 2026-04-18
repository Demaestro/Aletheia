import type { ReactNode } from "react";
import { RadioTower, Search, Globe } from "lucide-react";
import { useTranslation } from "react-i18next";
import { navItems, productName } from "../data/production";
import type { DesktopRuntimeStatus, ScreenKey, ScriptureCandidate } from "../types";
import { ActionButton, OutputCanvas, cn } from "./Primitives";
import { ServicePlanTimeline } from "./ServicePlanTimeline";

// Small inline badge for the dark topbar context
function TopBadge({ label, tone }: { label: string; tone: "ok" | "warn" | "neutral" }) {
  return (
    <span className={cn(
      "flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-[11px] font-medium",
      tone === "ok"      && "border-emerald-500/30 bg-emerald-500/12 text-emerald-400",
      tone === "warn"    && "border-amber-500/30   bg-amber-500/12   text-amber-400",
      tone === "neutral" && "border-white/12        bg-white/6         text-white/50",
    )}>
      <span className="h-1.5 w-1.5 rounded-full bg-current" />
      {label}
    </span>
  );
}

export function WorkspaceShell({
  active,
  onNavigate,
  children,
  title,
  preview,
  live,
  armed,
  desktopStatus,
  onToggleArmed,
  onSendLive,
  onManualSearch,
}: {
  active: ScreenKey;
  onNavigate: (screen: ScreenKey) => void;
  children: ReactNode;
  title: string;
  preview: ScriptureCandidate;
  live: ScriptureCandidate;
  armed: boolean;
  desktopStatus?: DesktopRuntimeStatus;
  onToggleArmed: () => void;
  onSendLive: () => void;
  onManualSearch?: () => void;
}) {
  const isDesktop = desktopStatus?.mode === "tauri";
  const { t, i18n } = useTranslation();

  return (
    <div className="flex h-screen overflow-hidden bg-gradient-to-br from-slate-900 via-paper to-black text-ink font-sans">

      {/* ── Left sidebar ─────────────────────────────────────────────── */}
      <aside className="relative flex w-[210px] shrink-0 flex-col border-r border-white/5 bg-white/[0.015] backdrop-blur-3xl">

        {/* Logo */}
        <button
          type="button"
          onClick={() => onNavigate("landing")}
          className="flex items-center gap-2.5 px-4 py-4 focus-visible:outline-none"
        >
          <span className="grid h-7 w-7 shrink-0 place-items-center rounded-[6px] bg-indigo-600 text-[11px] font-bold text-white">
            AL
          </span>
          <span className="text-sm font-semibold tracking-tight text-white/85">{productName}</span>
        </button>

        {/* Core status strip */}
        <div className="mx-3 mb-2 flex items-center gap-2 rounded-[6px] bg-white/[0.05] px-3 py-1.5">
          <span className={cn(
            "h-1.5 w-1.5 shrink-0 rounded-full",
            isDesktop ? "bg-emerald-400" : "bg-amber-400"
          )} />
          <span className="truncate text-[11px] text-white/45">
            {isDesktop ? t("status.coreOnline") : t("status.browserMode")}
          </span>
        </div>

        {/* Nav */}
        <nav className="flex-1 space-y-px px-2" aria-label="Operator screens">
          {navItems.map((item) => {
            const Icon = item.icon;
            const isActive = active === item.key;
            return (
              <button
                key={item.key}
                type="button"
                onClick={() => onNavigate(item.key)}
                className={cn(
                  "flex w-full items-center gap-2.5 rounded-[6px] px-2.5 py-[7px] text-left text-[13px] transition-colors",
                  isActive
                    ? "bg-gradient-to-r from-violet-600 to-indigo-600 font-semibold text-white shadow-neon shadow-violet-500/20"
                    : "font-medium text-white/50 hover:bg-white/[0.06] hover:text-white/90"
                )}
                aria-current={isActive ? "page" : undefined}
              >
                <Icon className="h-[14px] w-[14px] shrink-0" aria-hidden="true" />
                <span className="truncate">{t(`navigation.${item.key}.title`, { defaultValue: item.label })}</span>
              </button>
            );
          })}
        </nav>

        {/* Footer meta */}
        <div className="border-t border-white/[0.07] px-4 py-2.5">
          <p className="truncate text-[10px] text-white/25">
            {desktopStatus?.serviceSession ?? "loading…"}
          </p>
          <p className="mt-0.5 truncate text-[10px] text-white/20">
            {isDesktop ? `${desktopStatus?.auditCount ?? 0} audit events` : "Offline safe"}
          </p>
        </div>
      </aside>

      {/* ── Main ────────────────────────────────────────────────────────── */}
      <main className="flex flex-1 min-w-0 flex-col overflow-hidden">

        {/* Slim topbar */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-light/5 bg-transparent px-6">
          <h1 className="text-[14px] font-semibold tracking-tight text-white/90">{title}</h1>
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-1.5 mr-2 rounded-md border border-white/10 bg-white/5 px-2.5 py-1 text-[11px] shadow-glass backdrop-blur-md transition-colors hover:bg-white/10">
              <Globe className="h-3 w-3 text-white/60" />
              <select
                className="bg-transparent font-medium text-white/80 outline-none hover:text-white"
                value={i18n.language}
                onChange={(e) => i18n.changeLanguage(e.target.value)}
              >
                <option value="en" className="bg-mist text-white">English</option>
                <option value="ig" className="bg-mist text-white">Igbo</option>
                <option value="yo" className="bg-mist text-white">Yorùbá</option>
                <option value="ha" className="bg-mist text-white">Hausa</option>
              </select>
            </div>
            <TopBadge label={isDesktop ? "Desktop" : "Browser"} tone={isDesktop ? "ok" : "warn"} />
            {desktopStatus?.dataMiserEnabled && (
              <TopBadge label="Data miser" tone="warn" />
            )}
            {onManualSearch && (
              <button
                type="button"
                onClick={onManualSearch}
                className="flex items-center gap-1.5 rounded-[6px] px-2.5 py-1 text-[12px] font-medium text-white/50 transition-colors hover:bg-white/10 hover:text-white"
                aria-label="Manual scripture search"
              >
                <Search className="h-3.5 w-3.5" aria-hidden="true" />
                Search
              </button>
            )}
          </div>
        </header>

        {/* Service plan timeline strip */}
        <div className="flex shrink-0 items-center border-b border-white/[0.04] bg-white/[0.01] px-6 py-1.5">
          <ServicePlanTimeline />
        </div>

        {/* Page canvas — transparent back into the slate gradient base */}
        <div className="flex-1 overflow-y-auto bg-transparent p-6">
          {children}
        </div>
      </main>

      {/* ── Production rail ──────────────────────────────────────────────── */}
      <aside className="relative flex w-[280px] shrink-0 flex-col border-l border-white/5 bg-white/[0.015] backdrop-blur-3xl shadow-[-10px_0_30px_rgba(0,0,0,0.2)]">

        {/* Rail header */}
        <div className="flex items-center justify-between border-b border-white/[0.07] px-4 py-3">
          <span className="text-[10px] font-semibold uppercase tracking-widest text-white/30">
            Production
          </span>
          <div className="flex items-center gap-1.5">
            <span className={cn(
              "h-1.5 w-1.5 rounded-full transition-colors",
              armed ? "bg-rose-400" : "bg-white/20"
            )} />
            <span className={cn(
              "text-[11px] font-medium",
              armed ? "text-rose-400" : "text-white/30"
            )}>
              {armed ? "Armed" : "Hold"}
            </span>
          </div>
        </div>

        {/* Output canvases */}
        <div className="flex flex-1 flex-col gap-2.5 overflow-y-auto p-3">

          <div>
            <p className="mb-1.5 px-0.5 text-[10px] font-semibold uppercase tracking-widest text-white/30">
              Preview
            </p>
            <OutputCanvas label="" candidate={preview} state="preview" />
          </div>

          <div>
            <p className="mb-1.5 px-0.5 text-[10px] font-semibold uppercase tracking-widest text-white/30">
              Live
            </p>
            <div className={cn(
              "rounded-[6px] ring-1 transition-colors",
              armed ? "ring-rose-500/50" : "ring-transparent"
            )}>
              <OutputCanvas label="" candidate={live} state="live" />
            </div>
          </div>
        </div>

        {/* Send controls */}
        <div className="shrink-0 border-t border-white/[0.07] p-3 space-y-2">
          <div className="grid grid-cols-2 gap-2">
            <ActionButton tone="secondary" onClick={onToggleArmed}>
              {armed ? "Disarm" : "Arm"}
            </ActionButton>
            <ActionButton onClick={onSendLive} disabled={!armed}>
              <RadioTower className="mr-1.5 h-3 w-3" aria-hidden="true" />
              Go live
            </ActionButton>
          </div>
          <p className="text-center text-[10px] leading-relaxed text-white/22">
            {armed
              ? "Armed — operator action required to transmit."
              : "Arm destinations before sending live."}
          </p>
        </div>
      </aside>
    </div>
  );
}
