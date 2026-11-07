import { useState } from "react";
import { Command } from "cmdk";
import { Popover } from "radix-ui";
import type { ProjectInfo } from "../api";

/** The top-bar project switcher: a searchable combobox (Popover + Command, as in shadcn's combobox). */
export function ProjectMenu({
  projects,
  value,
  title,
  onChange,
}: {
  projects: ProjectInfo[];
  value: string;
  title?: string;
  onChange: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const current = projects.find((p) => p.id === value);
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger className="project-trigger" role="combobox" aria-expanded={open} aria-label="Project" title={title} data-value={value}>
        <span className="project-trigger-name">{current?.name ?? "Choose a project"}</span>
        <svg className="project-chevron" width="12" height="12" viewBox="0 0 12 12" aria-hidden>
          <path d="M3 4.5 6 7.5 9 4.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content className="menu" align="start" sideOffset={6}>
          <Command loop>
            <Command.Input className="menu-search" placeholder="Search projects…" autoFocus />
            <Command.List className="menu-list">
              <Command.Empty className="menu-empty">No project matches.</Command.Empty>
              {projects.map((p) => (
                <Command.Item
                  key={p.id}
                  value={`${p.name} ${p.id}`}
                  className="menu-item"
                  data-checked={p.id === value}
                  onSelect={() => {
                    setOpen(false);
                    if (p.id !== value) onChange(p.id);
                  }}
                >
                  <span className="menu-check">
                    {p.id === value && (
                      <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
                        <path d="m2.5 6.5 2.5 2.5 4.5-5.5" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
                      </svg>
                    )}
                  </span>
                  {p.name}
                </Command.Item>
              ))}
            </Command.List>
          </Command>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
