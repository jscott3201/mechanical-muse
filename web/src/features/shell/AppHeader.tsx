import { Building2, Cpu, LoaderCircle, Moon, Network, Server, ServerOff, Sparkles, Sun, Workflow } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { Resource } from "@/hooks/useResource";
import type { Theme } from "@/hooks/useTheme";
import type { Health } from "@/types/generated/Health";

function Chip({ icon, label, detail, tone = "outline" }: { icon: React.ReactNode; label: string; detail: string; tone?: "outline" | "secondary" | "destructive" }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant={tone} className="cursor-default">
          {icon}
          {label}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>{detail}</TooltipContent>
    </Tooltip>
  );
}

/** Product name, what the server can do right now, and the theme switch. */
export function AppHeader({ health, theme, onToggleTheme }: { health: Resource<Health>; theme: Theme; onToggleTheme: () => void }) {
  return (
    <header className="sticky top-0 z-20 border-b bg-card/90 backdrop-blur">
      <div className="mx-auto flex max-w-[1600px] flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3">
        <div className="flex items-center gap-2">
          <span className="grid size-8 place-items-center rounded-lg bg-primary text-primary-foreground" aria-hidden="true">
            <Workflow className="size-4" />
          </span>
          <div className="leading-tight">
            <h1 className="text-base font-semibold">Mechanical Muse</h1>
            <p className="text-xs text-muted-foreground">The muse proposes. The engine checks.</p>
          </div>
        </div>
        <div className="flex flex-1 flex-wrap items-center gap-2" aria-label="Server status">
          {health.status === "loading" && (
            <Badge variant="secondary">
              <LoaderCircle aria-hidden="true" className="animate-spin" /> connecting
            </Badge>
          )}
          {health.status === "error" && (
            <Badge variant="destructive">
              <ServerOff aria-hidden="true" /> server offline
            </Badge>
          )}
          {health.status === "ready" && (
            <>
              <Chip
                icon={<Server aria-hidden="true" />}
                label={`engine ${health.data.engine_rev.slice(0, 7)}`}
                detail="open-control-engine commit every graph is checked against"
              />
              {health.data.live_model ? (
                <Chip icon={<Sparkles aria-hidden="true" />} label={health.data.live_model} detail="Live Muse Spark authoring is available" />
              ) : (
                <Chip
                  icon={<Sparkles aria-hidden="true" />}
                  label={health.data.recorded_model ? "recorded model only" : "no model"}
                  tone="secondary"
                  detail={health.data.recorded_model ? "No model key on the server: authoring plays back recorded sessions, labelled as such" : "Authoring is off on this server"}
                />
              )}
              {health.data.boptest ? (
                <Chip icon={<Building2 aria-hidden="true" />} label="BOPTEST" detail="Live runs drive the BOPTEST office building" />
              ) : (
                <Chip icon={<Building2 aria-hidden="true" />} label="BOPTEST off" tone="secondary" detail="Start the server with MM_BOPTEST_URL for live runs; replays still work" />
              )}
              {health.data.bacnet_device !== undefined ? (
                <Chip icon={<Network aria-hidden="true" />} label={`BACnet device ${health.data.bacnet_device}`} detail="Live runs are mirrored read-only over BACnet/IP" />
              ) : (
                <Chip icon={<Network aria-hidden="true" />} label="BACnet off" tone="secondary" detail="Start the server with MM_BACNET_ADDR to mirror live runs" />
              )}
              <Chip icon={<Cpu aria-hidden="true" />} label={`${health.data.specs} sequences`} detail="Validated specs this server serves" />
            </>
          )}
        </div>
        <Button size="sm" variant="ghost" onClick={onToggleTheme} aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}>
          {theme === "dark" ? <Sun aria-hidden="true" /> : <Moon aria-hidden="true" />}
        </Button>
      </div>
    </header>
  );
}
