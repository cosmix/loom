import { Untrusted } from "@/plan/chat/visible-controls";

/** A failed action's message; `role="alert"` announces it, which a HazardPanel does not. */
export function ActionError({ message }: { message: string }) {
  return (
    <p role="alert" className="text-destructive [overflow-wrap:anywhere]">
      <Untrusted text={message} />
    </p>
  );
}
