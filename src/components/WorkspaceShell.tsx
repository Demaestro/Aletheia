import { memo, type ReactNode } from "react";
import { Cog, Globe, Moon, Search, Sun } from "lucide-react";
import { useTranslation } from "react-i18next";
import { navItems, productName } from "../data/production";
import { setOperatingMode } from "../services/desktopApi";
import { useDesktopStore } from "../store/useDesktopStore";
import { useThemeStore } from "../store/useThemeStore";
import type { DesktopRuntimeStatus, OperatingMode, ScreenKey, ScriptureCandidate } from "../types";
import { cn } from "./Primitives";
import { ServicePlanTimeline } from "./ServicePlanTimeline";

function TopBadge({ label, tone }: { label: string; tone: "ok" | "warn" | "neutral" }) {
  return (
    <span
      className={cn(
        "flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-[11px] font-medium",
        tone === "ok" && "border-emerald-500/35 bg-emerald-500/10 text-emerald-400",
        tone === "warn" && "border-amber-500/35 bg-amber-500/10 text-amber-400",
        tone === "neutral" && "border-line bg-mist text-muted",
      )}
    >
      <span className="h-1.5 w-1.5 rounded-full bg-current" />
      {label}
    </span>
  );
}

export const WorkspaceShell = memo(function WorkspaceShell({
  active,
  onNavigate,
  children,
  title,
  armed,
  desktopStatus,
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
  const themeMode = useThemeStore((s) => s.themeMode);
  const setThemeMode = useThemeStore((s) => s.setThemeMode);
  const setDesktopStatus = useDesktopStore((s) => s.setDesktopStatus);
  const operatingMode: OperatingMode = desktopStatus?.operatingMode ?? "assisted";
  const resolvedDark =
    themeMode === "dark" ||
    (themeMode === "system" &&
      typeof window !== "undefined" &&
      window.matchMedia?.("(prefers-color-scheme: dark)").matches);

  const handleModeChange = async (next: OperatingMode) => {
    try {
      const status = await setOperatingMode(next);
      setDesktopStatus(status);
    } catch (err) {
      console.warn("[mode] failed to set operating mode", err);
    }
  };

  return (
    <div className="flex h-screen overflow-hidden bg-[var(--c-app-bg)] text-ink font-sans">
      <aside className="flex w-[218px] shrink-0 flex-col border-r border-line bg-paper">
        <button
          type="button"
          onClick={() => onNavigate("landing")}
          className="flex items-center gap-2.5 px-4 py-4 focus-visible:outline-none focus-visible:outline-2 focus-visible:outline-accent"
        >
          <span className="grid h-7 w-7 shrink-0 place-items-center rounded-[6px] border border-accent/40 bg-accent/10 text-[11px] font-bold text-accent">
            AL
          </span>
          <span className="truncate text-sm font-semibold tracking-tight text-ink">{productName}</span>
        </button>

        <div className="mx-3 mb-2 flex items-center gap-2 rounded-[6px] border border-line bg-mist px-3 py-1.5">
          <span className={cn("h-1.5 w-1.5 shrink-0 rounded-full", isDesktop ? "bg-emerald-400" : "bg-amber-400")} />
          <span className="truncate text-[11px] text-muted">
            {isDesktop ? t("status.coreOnline") : t("status.browserMode")}
          </span>
        </div>

        <nav className="flex-1 space-y-1 px-2" aria-label="Operator screens">
          {navItems.map((item) => {
            const Icon = item.icon;
            const isActive = active === item.key;
            return (
              <button
                key={item.key}
                type="button"
                onClick={() => onNavigate(item.key)}
                className={cn(
                  "flex w-full items-center gap-2.5 rounded-[6px] border px-2.5 py-[7px] text-left text-[13px] transition-colors",
                  isActive
                    ? "border-accent/35 bg-accent/10 font-semibold text-ink"
                    : "border-transparent font-medium text-muted hover:border-line hover:bg-mist hover:text-ink",
                )}
                aria-current={isActive ? "page" : undefined}
              >
                <Icon className="h-[14px] w-[14px] shrink-0" aria-hidden="true" />
                <span className="truncate">{t(`navigation.${item.key}.title`, { defaultValue: item.label })}</span>
              </button>
            );
          })}
        </nav>

        <div className="border-t border-line px-4 py-2.5">
          <p className="truncate text-[10px] text-muted">{desktopStatus?.serviceSession ?? "loading..."}</p>
          <p className="mt-0.5 truncate text-[10px] text-muted/70">
            {isDesktop ? `${desktopStatus?.auditCount ?? 0} audit events` : "Offline safe"}
          </p>
        </div>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-line bg-paper px-5">
          <h1 className="truncate text-[14px] font-semibold tracking-tight text-ink">{title}</h1>
          <div className="flex items-center gap-2.5">
            <TopBadge
              label={
                armed
                  ? t("common.outputArmed", { defaultValue: "Output armed" })
                  : t("common.outputHold", { defaultValue: "Output hold" })
              }
              tone={armed ? "warn" : "neutral"}
            />
            <div
              className="flex items-center gap-1.5 rounded-md border border-line bg-mist px-2.5 py-1 text-[11px] transition-colors hover:border-accent/35"
              title="Operating mode controls automatic scripture behavior."
            >
              <Cog className="h-3 w-3 text-muted" />
              <select
                className="bg-transparent font-medium text-ink outline-none"
                value={operatingMode}
                onChange={(event) => void handleModeChange(event.target.value as OperatingMode)}
                aria-label={t("common.operatingMode", { defaultValue: "Operating mode" })}
              >
                <option value="manual" className="bg-paper text-ink">Manual</option>
                <option value="assisted" className="bg-paper text-ink">Assisted</option>
                <option value="auto" className="bg-paper text-ink">Auto</option>
                <option value="rehearsal" className="bg-paper text-ink">Rehearsal</option>
                <option value="mock" className="bg-paper text-ink">Mock</option>
              </select>
            </div>
            <div className="flex items-center gap-1.5 rounded-md border border-line bg-mist px-2.5 py-1 text-[11px] transition-colors hover:border-accent/35">
              <Globe className="h-3 w-3 text-muted" />
              <select
                className="bg-transparent font-medium text-ink outline-none"
                value={i18n.language}
                onChange={(event) => void i18n.changeLanguage(event.target.value)}
                aria-label={t("common.interfaceLanguage", { defaultValue: "Interface language" })}
              >
                <option value="en" className="bg-paper text-ink">English</option>
                <option value="ig" className="bg-paper text-ink">Igbo</option>
                <option value="yo" className="bg-paper text-ink">Yoruba</option>
                <option value="ha" className="bg-paper text-ink">Hausa</option>
              </select>
            </div>
            <button
              type="button"
              onClick={() => setThemeMode(resolvedDark ? "light" : "dark")}
              className="flex items-center gap-1.5 rounded-[6px] border border-line bg-mist px-2.5 py-1 text-[12px] font-medium text-muted transition-colors hover:border-accent/35 hover:text-ink"
              aria-label={
                resolvedDark
                  ? t("common.switchToLight", { defaultValue: "Switch to light mode" })
                  : t("common.switchToDark", { defaultValue: "Switch to dark mode" })
              }
              title={
                resolvedDark
                  ? t("common.switchToLight", { defaultValue: "Switch to light mode" })
                  : t("common.switchToDark", { defaultValue: "Switch to dark mode" })
              }
            >
              {resolvedDark ? <Sun className="h-3.5 w-3.5" aria-hidden="true" /> : <Moon className="h-3.5 w-3.5" aria-hidden="true" />}
              <span>
                {resolvedDark
                  ? t("common.light", { defaultValue: "Light" })
                  : t("common.dark", { defaultValue: "Dark" })}
              </span>
            </button>
            <TopBadge
              label={
                isDesktop
                  ? t("common.desktop", { defaultValue: "Desktop" })
                  : t("common.browser", { defaultValue: "Browser" })
              }
              tone={isDesktop ? "ok" : "warn"}
            />
            {desktopStatus?.dataMiserEnabled ? (
              <TopBadge label={t("common.dataMiser", { defaultValue: "Data miser" })} tone="warn" />
            ) : null}
            {onManualSearch ? (
              <button
                type="button"
                onClick={onManualSearch}
                className="flex items-center gap-1.5 rounded-[6px] border border-transparent px-2.5 py-1 text-[12px] font-medium text-muted transition-colors hover:border-line hover:bg-mist hover:text-ink"
                aria-label={t("common.manualScriptureSearch", { defaultValue: "Manual scripture search" })}
              >
                <Search className="h-3.5 w-3.5" aria-hidden="true" />
                {t("common.search", { defaultValue: "Search" })}
              </button>
            ) : null}
          </div>
        </header>

        {active !== "dashboard" ? (
          <div className="flex shrink-0 items-center border-b border-line bg-paper px-5 py-1.5">
            <ServicePlanTimeline />
          </div>
        ) : null}

        <div className={cn("flex-1 overflow-y-auto bg-[var(--c-app-bg)]", active === "dashboard" ? "p-4" : "p-6")}>
          {children}
        </div>
      </main>
    </div>
  );
});
