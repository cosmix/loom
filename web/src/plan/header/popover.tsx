import { cn } from "cn";
import { Popover as PopoverPrimitive } from "radix-ui";
import type { ComponentProps } from "react";

function Popover(props: ComponentProps<typeof PopoverPrimitive.Root>) {
  return <PopoverPrimitive.Root data-slot="popover" {...props} />;
}

function PopoverTrigger(props: ComponentProps<typeof PopoverPrimitive.Trigger>) {
  return <PopoverPrimitive.Trigger data-slot="popover-trigger" {...props} />;
}

/**
 * Radix renders the content as `role="dialog"`, so every popover needs an accessible name.
 *
 * The surface is the raised `--popover` with the `--popover-border` edge and the floating shadow
 * (`--plan-shadow`, which the halo leads, over `--popover-lift`). A popover is not modal, so it
 * draws no scrim and no blur: the page under it stays live and readable, and a popover a question
 * opens on its own must not dim what the reader is working in.
 */
function PopoverContent({
  className,
  align = "end",
  sideOffset = 6,
  ...props
}: ComponentProps<typeof PopoverPrimitive.Content> & { "aria-label": string }) {
  return (
    <PopoverPrimitive.Portal>
      <PopoverPrimitive.Content
        data-slot="popover-content"
        align={align}
        sideOffset={sideOffset}
        className={cn(
          "z-50 flex w-80 max-w-[calc(100vw-2rem)] flex-col gap-3 rounded-xl bg-popover p-3 text-sm text-popover-foreground ring-1 ring-(--popover-border) shadow-[var(--plan-shadow),var(--popover-lift)] outline-none duration-100 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
          className,
        )}
        {...props}
      />
    </PopoverPrimitive.Portal>
  );
}

export { Popover, PopoverContent, PopoverTrigger };
