import { motion } from "framer-motion";
import { ArrowRight, MonitorUp, ShieldCheck } from "lucide-react";
import { heroImage, integrations, productName, serviceStats } from "../data/production";
import type { ScreenKey } from "../types";
import { ActionButton, Metric, StatusPill, fadeUp } from "./Primitives";

export function LandingPage({ onOpen }: { onOpen: (screen: ScreenKey) => void }) {
  return (
    <main className="min-h-screen bg-paper text-ink">
      <section
        className="relative flex min-h-[92svh] overflow-hidden bg-ink text-white"
        style={{
          backgroundImage: `linear-gradient(90deg, rgba(7,8,7,0.92), rgba(7,8,7,0.62) 48%, rgba(7,8,7,0.28)), url(${heroImage})`,
          backgroundPosition: "center",
          backgroundSize: "cover"
        }}
      >
        <div className="absolute inset-0 bg-[linear-gradient(180deg,rgba(7,8,7,0.2),rgba(7,8,7,0.72))]" />
        <div className="relative mx-auto flex w-full max-w-[1500px] flex-col px-7 py-7 lg:px-10">
          <nav className="flex items-center justify-between" aria-label="Landing navigation">
            <button
              type="button"
              onClick={() => onOpen("landing")}
              className="text-lg font-semibold tracking-tight text-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-white"
            >
              {productName}
            </button>
            <div className="hidden items-center gap-3 text-sm text-white/70 md:flex">
              <span>Local-first</span>
              <span>Offline worship</span>
              <span>Broadcast output</span>
            </div>
          </nav>

          <div className="grid flex-1 items-center gap-10 py-14 lg:grid-cols-[minmax(0,0.92fr)_minmax(520px,1fr)]">
            <motion.div initial="hidden" animate="visible" transition={{ staggerChildren: 0.08 }}>
              <motion.p variants={fadeUp} className="text-sm font-semibold uppercase tracking-[0.22em] text-white/70">
                Scripture detection for live worship teams
              </motion.p>
              <motion.h1
                variants={fadeUp}
                className="mt-5 max-w-3xl text-5xl font-semibold leading-[0.95] tracking-tight md:text-7xl"
              >
                Detect scripture. Approve with confidence. Present without internet.
              </motion.h1>
              <motion.p variants={fadeUp} className="mt-6 max-w-2xl text-lg leading-8 text-white/75">
                Built for church AV operators who need EasyWorship-compatible scripture output, OBS control, offline search, multilingual services, and a safe manual fallback.
              </motion.p>
              <motion.div variants={fadeUp} className="mt-8 flex flex-wrap gap-3">
                <ActionButton onClick={() => onOpen("dashboard")}>
                  Open operator console
                  <ArrowRight className="ml-2 h-4 w-4" aria-hidden="true" />
                </ActionButton>
                <ActionButton tone="secondary" onClick={() => onOpen("onboarding")} className="border-white/25 bg-white/10 text-white hover:border-white/60">
                  Run setup rehearsal
                </ActionButton>
              </motion.div>
            </motion.div>

            <motion.div
              initial={{ opacity: 0, y: 18 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.55, delay: 0.22 }}
              className="functional-panel hidden border border-white/15 bg-[#0D100F]/80 p-5 shadow-output backdrop-blur-xl lg:block"
              aria-label="Live production snapshot"
            >
              <div className="flex items-center justify-between border-b border-white/10 pb-4">
                <div>
                  <p className="text-xs uppercase tracking-[0.18em] text-white/50">Current service</p>
                  <p className="mt-1 text-xl font-semibold">Sunday AM rehearsal</p>
                </div>
                <StatusPill tone="armed" label="Preview armed" />
              </div>
              <div className="grid gap-4 py-5">
                <div className="rounded-[6px] border border-white/10 bg-white/[0.04] p-4">
                  <p className="text-sm text-white/60">Detected candidate</p>
                  <p className="mt-2 text-3xl font-semibold">Romans 8:28</p>
                  <p className="mt-3 max-w-xl text-sm leading-6 text-white/70">
                    Exact reference plus quoted phrase. Manual live required.
                  </p>
                </div>
                <div className="grid grid-cols-2 gap-3">
                  {integrations.slice(0, 4).map((integration) => (
                    <div key={integration.id} className="rounded-[6px] border border-white/10 px-3 py-3">
                      <p className="text-sm font-semibold">{integration.name}</p>
                      <p className="mt-1 text-xs text-white/50">{integration.detail}</p>
                    </div>
                  ))}
                </div>
              </div>
              <div className="flex items-center gap-3 border-t border-white/10 pt-4 text-sm text-white/70">
                <MonitorUp className="h-4 w-4" aria-hidden="true" />
                <span>Preview and live remain separate by default.</span>
              </div>
            </motion.div>
          </div>
        </div>
      </section>

      <section className="mx-auto grid max-w-[1500px] gap-6 px-7 py-12 md:grid-cols-4 lg:px-10">
        {serviceStats.map((stat) => (
          <Metric key={stat.label} {...stat} />
        ))}
      </section>

      <section className="border-y border-white/5 bg-mist">
        <div className="mx-auto grid max-w-[1500px] gap-10 px-7 py-14 lg:grid-cols-[0.7fr_1fr] lg:px-10">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">Design promise</p>
            <h2 className="mt-3 text-3xl font-semibold tracking-tight text-ink">Graceful degradation beats magic.</h2>
          </div>
          <div className="grid gap-4 md:grid-cols-3">
            {["Local scripture search stays instant.", "AI explains confidence before approval.", "Every output can be rehearsed before live."].map((copy) => (
              <div key={copy} className="border-l border-white/5 pl-4 text-sm leading-6 text-graphite">
                <ShieldCheck className="mb-3 h-5 w-5 text-accent" aria-hidden="true" />
                {copy}
              </div>
            ))}
          </div>
        </div>
      </section>
    </main>
  );
}
