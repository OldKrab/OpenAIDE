import {
  BookOpen,
  BrainCircuit,
  CloudDownload,
  Earth,
  FileInput,
  FileText,
  Pencil,
  Repeat2,
  Search,
  Terminal,
  Trash2,
  Wrench,
} from "lucide-react";

export function toolKindIcon(kind: string | undefined, size: number, className?: string) {
  if (kind === "skill") return <BookOpen className={className} size={size} />;
  if (kind === "read") return <FileText className={className} size={size} />;
  if (kind === "edit") return <Pencil className={className} size={size} />;
  if (kind === "delete") return <Trash2 className={className} size={size} />;
  if (kind === "move") return <FileInput className={className} size={size} />;
  if (kind === "search") return <Search className={className} size={size} />;
  if (kind === "web_search") return <Earth className={className} size={size} />;
  if (kind === "execute") return <Terminal className={className} size={size} />;
  if (kind === "think") return <BrainCircuit className={className} size={size} />;
  if (kind === "fetch") return <CloudDownload className={className} size={size} />;
  if (kind === "switch_mode") return <Repeat2 className={className} size={size} />;
  return <Wrench className={className} size={size} />;
}
