/**
 * Placeholder for pages owned by later phases. Renders an explicit "Not
 * implemented yet" state — never placeholder numbers.
 */
import { useT } from "@/lib/i18n";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export function PlaceholderPage({ title }: { title: string }) {
  const t = useT();
  return (
    <div className="grid gap-4 p-5">
      <Card data-testid="not-implemented">
        <CardHeader>
          <CardTitle>{title}</CardTitle>
        </CardHeader>
        <CardContent className="text-sm">
          <p className="font-medium text-muted">{t("common.notImplemented")}</p>
          <p className="mt-2 text-muted">{t("common.notImplementedHint")}</p>
        </CardContent>
      </Card>
    </div>
  );
}
