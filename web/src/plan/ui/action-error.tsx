import { Untrusted } from "@/plan/chat/visible-controls";

/**
 * A failed action's message, led by what failed when `title` names it; `role="alert"` announces
 * it, which a HazardPanel does not. Only `message` is drawn through Untrusted: the title is ours.
 */
export function ActionError({ message, title }: { message: string; title?: string }) {
  return (
    <p role="alert" className="text-destructive [overflow-wrap:anywhere]">
      {title && `${title}: `}
      <Untrusted text={message} />
    </p>
  );
}
