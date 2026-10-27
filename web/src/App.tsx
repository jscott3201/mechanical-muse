import { type ReactNode, useEffect, useState } from "react";

import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Overview } from "@/features/overview/Overview";
import { RunsPage } from "@/features/runs/RunsPage";
import { AppHeader } from "@/features/shell/AppHeader";
import { SpecDetail } from "@/features/SpecDetail";
import { SpecList } from "@/features/SpecList";
import { WorkbenchPage } from "@/features/workbench/WorkbenchPage";
import { useResource } from "@/hooks/useResource";
import { useRoute } from "@/hooks/useRoute";
import { useTheme } from "@/hooks/useTheme";
import type { Page, Route } from "@/lib/route";
import type { Health } from "@/types/generated/Health";

const PAGES: readonly Page[] = ["overview", "sequences", "workbench", "runs"];
const PAGE_TITLES: Record<Page, string> = {
  overview: "Overview",
  sequences: "Sequences",
  workbench: "Bring your own",
  runs: "Runs",
};

type SequencesRoute = Extract<Route, { page: "sequences" }>;
type RunsRoute = Extract<Route, { page: "runs" }>;

function isPage(v: string): v is Page {
  return (PAGES as readonly string[]).includes(v);
}

/**
 * A top-level tab that mounts on first visit and then stays mounted while hidden, so an
 * authoring stream, a reading or a receipt survives a look at another tab.
 */
function Pane({ value, visited, children }: { value: Page; visited: ReadonlySet<Page>; children: ReactNode }) {
  if (!visited.has(value)) return null;
  return (
    <TabsContent value={value} forceMount className="data-[state=inactive]:hidden">
      {children}
    </TabsContent>
  );
}

export function App() {
  const [route, go] = useRoute();
  const page = route.page;
  const health = useResource<Health>("/api/health");
  // Bumped when the workbench adds a sequence, so the list reloads.
  const [listVersion, setListVersion] = useState(0);
  const [theme, toggleTheme] = useTheme();
  const [visited, setVisited] = useState<ReadonlySet<Page>>(() => new Set([page]));
  if (!visited.has(page)) setVisited(new Set([...visited, page]));
  // Hidden tabs stay mounted, so each keeps the route it last had: a spec page keeps its spec
  // (and its authoring stream) while the Runs tab is open, and a tab click returns to it.
  const [seqRoute, setSeqRoute] = useState<SequencesRoute>(() => (route.page === "sequences" ? route : { page: "sequences" }));
  const [runsRoute, setRunsRoute] = useState<RunsRoute>(() => (route.page === "runs" ? route : { page: "runs" }));
  if (route.page === "sequences" && route !== seqRoute) setSeqRoute(route);
  if (route.page === "runs" && route !== runsRoute) setRunsRoute(route);
  const spec = seqRoute.spec ?? null;
  useEffect(() => {
    // A spec's page names itself once its title is known (SpecDetail).
    if (page !== "sequences" || !spec) document.title = `${PAGE_TITLES[page]} · Mechanical Muse`;
  }, [page, spec]);
  const tabRoute = (p: Page): Route => (p === "sequences" ? seqRoute : p === "runs" ? runsRoute : { page: p });
  return (
    <TooltipProvider>
      <div className="min-h-svh bg-background text-foreground">
        <AppHeader health={health} theme={theme} onToggleTheme={toggleTheme} />
        <Tabs
          value={page}
          onValueChange={(v) => {
            if (isPage(v)) go(tabRoute(v));
          }}
          className="mx-auto max-w-[1600px] gap-4 p-4"
        >
          <TabsList>
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="sequences">Sequences</TabsTrigger>
            <TabsTrigger value="workbench">Bring your own</TabsTrigger>
            <TabsTrigger value="runs">Runs</TabsTrigger>
          </TabsList>
          <Pane value="overview" visited={visited}>
            <Overview health={health.status === "ready" ? health.data : null} onGo={go} />
          </Pane>
          <Pane value="sequences" visited={visited}>
            <main className="grid gap-4 md:grid-cols-[17rem_minmax(0,1fr)]">
              <aside className="md:sticky md:top-20 md:self-start">
                <SpecList key={listVersion} selected={spec} />
              </aside>
              <section className="min-w-0">
                {spec ? (
                  <SpecDetail
                    key={spec}
                    name={spec}
                    active={page === "sequences"}
                    section={seqRoute.section}
                    onSection={(section) => go({ page: "sequences", spec, section })}
                    onRunDraft={(s, contentId) => go({ page: "runs", draft: { spec: s, contentId } })}
                  />
                ) : (
                  <div className="grid min-h-64 place-items-center rounded-xl border border-dashed p-8 text-center text-sm text-muted-foreground">
                    Choose a sequence to see its interface and graph, verify it, or have Muse Spark draft it.
                  </div>
                )}
              </section>
            </main>
          </Pane>
          <Pane value="workbench" visited={visited}>
            <WorkbenchPage
              onOpenSpec={(name) => {
                setListVersion((v) => v + 1);
                go({ page: "sequences", spec: name });
              }}
            />
          </Pane>
          <Pane value="runs" visited={visited}>
            <RunsPage
              initialDraft={runsRoute.draft ?? null}
              run={runsRoute.run ?? null}
              replay={runsRoute.replay ?? null}
              onSelectRun={(id) => go({ page: "runs", run: id })}
            />
          </Pane>
        </Tabs>
      </div>
    </TooltipProvider>
  );
}
