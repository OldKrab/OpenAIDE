import { Search } from "lucide-react";

/** Shared search affordance for the Settings catalog pages. */
export function SettingsCatalogSearch({
  label,
  onChange,
  placeholder,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  placeholder: string;
  value: string;
}) {
  return (
    <label className="settings-catalog-search">
      <Search aria-hidden="true" size={14} />
      <input
        aria-label={label}
        onChange={(event) => onChange(event.currentTarget.value)}
        placeholder={placeholder}
        type="search"
        value={value}
      />
    </label>
  );
}
