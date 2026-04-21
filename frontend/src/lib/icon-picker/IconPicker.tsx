/**
 * IconPicker — compact popover for selecting a Lucide icon.
 *
 * Uses a curated domain-relevant icon set with search + tooltip labels.
 * Trigger is a small square button showing the current icon.
 *
 * API: value (kebab-case name | undefined), onChange(name | undefined)
 */
import { useMemo, useState } from "react";
import {
  Activity, AlertTriangle, Archive, BarChart3, Battery, Bell, Bolt, Book,
  Box, Building, Building2, Calendar, Check, ChevronRight, CircleDot, Clock,
  Cloud, Cpu, Database, Droplets, Factory, FileText, Flame, Gauge, Globe,
  Grid3X3, HardDrive, Heart, Home, Key, Layers, Layout, LayoutDashboard,
  Link, List, Lock, Mail, Map, MapPin, Monitor, Network, Package, Plug,
  Power, Radio, RefreshCw, Server, Settings, Shield, Signal, Sliders,
  Smartphone, Star, Sun, Tag, Thermometer, Timer, ToggleLeft, Trash2,
  TrendingUp, Truck, Users, Wifi, Wrench, Zap,
  type LucideIcon,
} from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@listo/ui-kit";
import { ScrollArea } from "@listo/ui-kit";
import { Tooltip, TooltipContent, TooltipTrigger } from "@listo/ui-kit";
import { Button } from "@listo/ui-kit";
import { cn } from "@/lib/utils";

// ─── Curated icon map ────────────────────────────────────────────────────────

const ICON_MAP: Record<string, LucideIcon> = {
  Activity, AlertTriangle, Archive, BarChart3, Battery, Bell, Bolt, Book,
  Box, Building, Building2, Calendar, Check, ChevronRight, CircleDot, Clock,
  Cloud, Cpu, Database, Droplets, Factory, FileText, Flame, Gauge, Globe,
  Grid3X3, HardDrive, Heart, Home, Key, Layers, Layout, LayoutDashboard,
  Link, List, Lock, Mail, Map, MapPin, Monitor, Network, Package, Plug,
  Power, Radio, RefreshCw, Server, Settings, Shield, Signal, Sliders,
  Smartphone, Star, Sun, Tag, Thermometer, Timer, ToggleLeft, Trash2,
  TrendingUp, Truck, Users, Wifi, Wrench, Zap,
};

const ICON_NAMES = Object.keys(ICON_MAP);

// ─── DynamicIcon (exported for use in FlowCanvas etc.) ─────────────────────

export function DynamicIcon({ name, className }: { name: string; className?: string | undefined }) {
  const Icon = ICON_MAP[name];
  if (Icon) return <Icon className={cn("h-4 w-4", className)} />;
  return <span className={className}>{name}</span>;
}

export function isKnownIcon(name: string): boolean {
  return name in ICON_MAP;
}

// ─── toPascal helper (re-exported for FlowCanvas DynIcon) ──────────────────
export function toPascal(kebab: string): string {
  return kebab
    .replace(/-([a-z0-9])/g, (_, c: string) => c.toUpperCase())
    .replace(/^([a-z])/, (_, c: string) => c.toUpperCase());
}

export function getAllIconNames(): readonly string[] {
  return ICON_NAMES;
}

// ─── IconPicker ──────────────────────────────────────────────────────────────

export interface IconPickerProps {
  value?: string | undefined;
  onChange: (name: string | undefined) => void;
  className?: string | undefined;
  disabled?: boolean | undefined;
}

export function IconPicker({ value, onChange, className, disabled }: IconPickerProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");

  const filtered = useMemo(() => {
    if (!search) return ICON_NAMES;
    const q = search.toLowerCase();
    return ICON_NAMES.filter((n) => n.toLowerCase().includes(q));
  }, [search]);

  function handleSelect(name: string) {
    onChange(name);
    setOpen(false);
    setSearch("");
  }

  function handleClear() {
    onChange(undefined);
    setOpen(false);
    setSearch("");
  }

  const CurrentIcon = value ? ICON_MAP[value] : undefined;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={disabled}
          className={cn(
            "h-9 w-9 p-0 flex items-center justify-center",
            !value && "text-muted-foreground",
            className,
          )}
        >
          {CurrentIcon ? (
            <CurrentIcon className="h-4 w-4" />
          ) : (
            <span className="text-xs font-medium">Aa</span>
          )}
        </Button>
      </PopoverTrigger>

      <PopoverContent className="w-64 p-0" align="start" sideOffset={4}>
        {/* Search */}
        <div className="border-b border-border p-2">
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search icons…"
            autoFocus
            className={cn(
              "h-7 w-full rounded-md border border-input bg-background px-2 text-xs",
              "outline-none focus-visible:border-ring focus-visible:ring-[2px] focus-visible:ring-ring/50",
              "placeholder:text-muted-foreground",
            )}
          />
        </div>

        {/* Grid */}
        <ScrollArea className="h-48">
          <div className="grid grid-cols-8 gap-0.5 p-2">
            {filtered.map((name) => {
              const Icon = ICON_MAP[name]!;
              const isSelected = value === name;
              return (
                <Tooltip key={name}>
                  <TooltipTrigger asChild>
                    <button
                      type="button"
                      onClick={() => handleSelect(name)}
                      className={cn(
                        "flex h-7 w-7 items-center justify-center rounded transition-colors",
                        isSelected
                          ? "bg-primary text-primary-foreground"
                          : "text-muted-foreground hover:bg-muted hover:text-foreground",
                      )}
                    >
                      <Icon className="h-3.5 w-3.5" />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="bottom" className="text-xs">
                    {name}
                  </TooltipContent>
                </Tooltip>
              );
            })}
            {filtered.length === 0 && (
              <p className="col-span-8 py-4 text-center text-xs text-muted-foreground">
                No icons match &ldquo;{search}&rdquo;
              </p>
            )}
          </div>
        </ScrollArea>

        {/* Clear */}
        {value && (
          <div className="border-t border-border p-1.5">
            <button
              type="button"
              onClick={handleClear}
              className="w-full rounded-md py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
            >
              Clear icon
            </button>
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
