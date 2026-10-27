import { useState } from "react";

import { Badge } from "@/components/ui/badge";
import { SpecDetail } from "@/features/SpecDetail";
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
        {health.status === "ready" ? (
          <Badge variant="outline">engine {health.data.engine_rev.slice(0, 7)}</Badge>
        ) : (
          <Badge variant="destructive">{health.status === "error" ? "server offline" : "connecting"}</Badge>
        )}
      </header>
      <main className="grid gap-4 p-4 md:grid-cols-[16rem_minmax(0,1fr)]">
        <aside>
          <SpecList selected={spec} onSelect={setSpec} />
        </aside>
        <section className="min-w-0">
          {spec ? (
            <SpecDetail name={spec} />
          ) : (
            <p className="text-sm text-muted-foreground">Choose a sequence to see its interface and verify it.</p>
          )}
        </section>
      </main>
    </div>
  );
}
