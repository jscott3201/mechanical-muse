import { LoaderCircle, Server, ServerOff } from "lucide-react";
import { useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { SpecDetail } from "@/features/SpecDetail";
import { RunsPage } from "@/features/runs/RunsPage";
import { SpecList } from "@/features/SpecList";
import { useResource } from "@/hooks/useResource";
import type { Health } from "@/types/generated/Health";

export function App() {
  const [spec, setSpec] = useState<string | null>(null);
  const health = useResource<Health>("/api/health");
  return (
    <div className="min-h-svh bg-background text-foreground">
      <header className="flex flex-wrap items-center gap-3 border-b px-4 py-3">
        <h1 className="text-base font-semibold">Mechanical Muse</h1>
        {health.status === "ready" && (
          <Badge variant="outline">
            <Server aria-hidden="true" /> engine {health.data.engine_rev.slice(0, 7)}
          </Badge>
        )}
        {health.status === "loading" && (
          <Badge variant="secondary">
            <LoaderCircle aria-hidden="true" /> connecting
          </Badge>
        )}
        {health.status === "error" && (
          <Badge variant="destructive">
            <ServerOff aria-hidden="true" /> server offline
          </Badge>
        )}
      </header>
      <Tabs defaultValue="sequences" className="p-4">
        <TabsList>
          <TabsTrigger value="sequences">Sequences</TabsTrigger>
          <TabsTrigger value="runs">Runs</TabsTrigger>
        </TabsList>
        <TabsContent value="sequences">
          <main className="grid gap-4 pt-2 md:grid-cols-[16rem_minmax(0,1fr)]">
            <aside>
              <SpecList selected={spec} onSelect={setSpec} />
            </aside>
            <section className="min-w-0">
              {spec ? (
                <SpecDetail key={spec} name={spec} />
              ) : (
                <p className="text-sm text-muted-foreground">Choose a sequence to see its interface and verify it.</p>
              )}
            </section>
          </main>
        </TabsContent>
        <TabsContent value="runs" className="pt-2">
          <RunsPage />
        </TabsContent>
      </Tabs>
    </div>
  );
}
