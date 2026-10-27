import { useState } from "react";

import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { TooltipProvider } from "@/components/ui/tooltip";
import { type Destination, Overview } from "@/features/overview/Overview";
import { RunsPage } from "@/features/runs/RunsPage";
import { AppHeader } from "@/features/shell/AppHeader";
import { SpecDetail } from "@/features/SpecDetail";
import { SpecList } from "@/features/SpecList";
import { WorkbenchPage } from "@/features/workbench/WorkbenchPage";
import { useResource } from "@/hooks/useResource";
import { useTheme } from "@/hooks/useTheme";
import type { Health } from "@/types/generated/Health";

type Tab = "overview" | "workbench" | Destination;

export function App() {
  const [tab, setTab] = useState<Tab>("overview");
  const [spec, setSpec] = useState<string | null>(null);
  // A verified draft chosen from the authoring panel, handed to the run form.
  const [runDraft, setRunDraft] = useState<{ spec: string; contentId: string } | null>(null);
  const health = useResource<Health>("/api/health");
  // Bumped when the workbench adds a sequence, so the list reloads.
  const [listVersion, setListVersion] = useState(0);
  const [theme, toggleTheme] = useTheme();
  return (
    <TooltipProvider>
      <div className="min-h-svh bg-background text-foreground">
        <AppHeader health={health} theme={theme} onToggleTheme={toggleTheme} />
        <Tabs value={tab} onValueChange={(v) => setTab(v as Tab)} className="mx-auto max-w-[1600px] gap-4 p-4">
          <TabsList>
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="sequences">Sequences</TabsTrigger>
            <TabsTrigger value="workbench">Your sequence</TabsTrigger>
            <TabsTrigger value="runs">Runs</TabsTrigger>
          </TabsList>
          <TabsContent value="overview">
            <Overview health={health.status === "ready" ? health.data : null} onGo={setTab} />
          </TabsContent>
          <TabsContent value="sequences">
            <main className="grid gap-4 md:grid-cols-[17rem_minmax(0,1fr)]">
              <aside className="md:sticky md:top-20 md:self-start">
                <SpecList key={listVersion} selected={spec} onSelect={setSpec} />
              </aside>
              <section className="min-w-0">
                {spec ? (
                  <SpecDetail
                    key={spec}
                    name={spec}
                    onRunDraft={(s, contentId) => {
                      setRunDraft({ spec: s, contentId });
                      setTab("runs");
                    }}
                  />
                ) : (
                  <div className="grid min-h-64 place-items-center rounded-xl border border-dashed p-8 text-center text-sm text-muted-foreground">
                    Choose a sequence to see its interface and graph, verify it, or have Muse Spark draft it.
                  </div>
                )}
              </section>
            </main>
          </TabsContent>
          <TabsContent value="workbench">
            <WorkbenchPage
              onOpenSpec={(name) => {
                setListVersion((v) => v + 1);
                setSpec(name);
                setTab("sequences");
              }}
            />
          </TabsContent>
          <TabsContent value="runs">
            <RunsPage initialDraft={runDraft} />
          </TabsContent>
        </Tabs>
      </div>
    </TooltipProvider>
  );
}
