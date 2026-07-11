import { BookOpen, Earth, FileText, Globe, Pencil, Search, Terminal, Wrench } from "lucide-react";

export function toolKindIcon(kind: string | undefined, size: number, className?: string) {
  if (kind === "skill") return <BookOpen className={className} size={size} />;
  if (kind === "read") return <FileText className={className} size={size} />;
  if (kind === "edit") return <Pencil className={className} size={size} />;
  if (kind === "search") return <Search className={className} size={size} />;
  if (kind === "web_search") return <Earth className={className} size={size} />;
  if (kind === "fetch") return <Globe className={className} size={size} />;
  if (kind === "execute") return <Terminal className={className} size={size} />;
  return <Wrench className={className} size={size} />;
}
