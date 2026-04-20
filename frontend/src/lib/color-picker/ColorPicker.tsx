/**
 * ColorPicker — preset swatches + custom color input.
 *
 * Based on the reference implementation in rubix/frontend.
 *
 * Props:
 *   value     – current hex color or undefined
 *   onChange  – called with new hex string, or "" to clear
 *   showCustom – show the gradient "custom" button (default true)
 *   showClear  – show the Clear button when a value is set (default true)
 */
import { cn } from "@/lib/utils";

export interface ColorPickerProps {
  value?: string | undefined;
  onChange: (color: string) => void;
  showCustom?: boolean | undefined;
  showClear?: boolean | undefined;
  className?: string | undefined;
}

const PRESETS = [
  "#ef4444", // red
  "#f97316", // orange
  "#f59e0b", // amber
  "#eab308", // yellow
  "#84cc16", // lime
  "#22c55e", // green
  "#14b8a6", // teal
  "#06b6d4", // cyan
  "#3b82f6", // blue
  "#6366f1", // indigo
  "#8b5cf6", // violet
  "#ec4899", // pink
];

export function ColorPicker({
  value,
  onChange,
  showCustom = true,
  showClear = true,
  className,
}: ColorPickerProps) {
  return (
    <div className={cn("space-y-3", className)}>
      {/* Preset swatches + optional custom picker */}
      <div className="grid grid-cols-6 gap-2">
        {PRESETS.map((color) => (
          <button
            key={color}
            type="button"
            title={color}
            onClick={() => onChange(color)}
            className="h-9 w-full rounded-md border-2 transition-all hover:scale-110"
            style={{
              backgroundColor: color,
              borderColor: value === color ? "white" : "transparent",
              boxShadow: value === color ? `0 0 0 2px ${color}` : "none",
            }}
          />
        ))}

        {/* Custom / full-range picker */}
        {showCustom && (
          <label
            title="Custom color"
            className="relative flex h-9 w-full cursor-pointer items-center justify-center overflow-hidden rounded-md border-2 border-border transition-all hover:scale-110"
            style={{
              background:
                "linear-gradient(135deg, #ef4444, #f97316, #eab308, #22c55e, #3b82f6, #8b5cf6)",
            }}
          >
            <div className="absolute inset-0 flex items-center justify-center bg-background/75 backdrop-blur-[2px]">
              <svg className="h-4 w-4 text-foreground" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                  d="M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.343M11 7.343l1.657-1.657a2 2 0 012.828 0l2.829 2.829a2 2 0 010 2.828l-8.486 8.485M7 17h.01" />
              </svg>
            </div>
            <input
              type="color"
              value={value && value.startsWith("#") ? value : "#6366f1"}
              onChange={(e) => onChange(e.target.value)}
              className="sr-only"
            />
          </label>
        )}
      </div>

      {/* Hex text input + clear */}
      <div className="flex items-center gap-2">
        <input
          type="text"
          value={value ?? ""}
          onChange={(e) => onChange(e.target.value)}
          placeholder="#3b82f6"
          className={cn(
            "h-9 flex-1 rounded-md border border-input bg-background px-3 text-xs",
            "outline-none focus-visible:border-ring focus-visible:ring-[2px] focus-visible:ring-ring/50",
            "placeholder:text-muted-foreground",
          )}
        />
        {showClear && value && (
          <button
            type="button"
            onClick={() => onChange("")}
            className="h-9 rounded-md border border-border px-3 text-xs text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            Clear
          </button>
        )}
      </div>
    </div>
  );
}
