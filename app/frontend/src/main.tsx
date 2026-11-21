import React, { useEffect, useState, useCallback, useRef } from "react";
import { createRoot } from "react-dom/client";
import "@astryxdesign/core/reset.css";
import "@astryxdesign/core/astryx.css";
import { Theme } from "@astryxdesign/core/theme";
import { themes, defaultThemeSlug } from "../astryx-themes";
import { AppShell } from "@astryxdesign/core/AppShell";
import { Layout, LayoutContent } from "@astryxdesign/core/Layout";
import {
  SideNav,
  SideNavHeading,
  SideNavItem,
  SideNavSection,
} from "@astryxdesign/core/SideNav";
import { VStack, HStack, StackItem } from "@astryxdesign/core/Stack";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { Link } from "@astryxdesign/core/Link";
import { Text } from "@astryxdesign/core/Text";
import { Icon } from "@astryxdesign/core/Icon";
import { Thumbnail } from "@astryxdesign/core/Thumbnail";
import brandIcon from "./brand-icon.svg";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Button } from "@astryxdesign/core/Button";
import { Switch } from "@astryxdesign/core/Switch";
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { TextInput } from "@astryxdesign/core/TextInput";
import { Selector } from "@astryxdesign/core/Selector";
import { ContextMenu } from "@astryxdesign/core/ContextMenu";
import { MoreMenu } from "@astryxdesign/core/MoreMenu";
import { Dialog, DialogHeader } from "@astryxdesign/core/Dialog";
import { Collapsible } from "@astryxdesign/core/Collapsible";
import { Divider } from "@astryxdesign/core/Divider";
import { Table, pixel, proportional } from "@astryxdesign/core/Table";
import type { TableColumn } from "@astryxdesign/core/Table";
import { Card } from "@astryxdesign/core/Card";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import { Banner } from "@astryxdesign/core/Banner";
import {
  LayoutGrid,
  Monitor,
  BookOpen,
  Play,
  Square,
  Plus,
  Star,
  EyeOff,
  Search,
  ExternalLink,
  RotateCcw,
  Settings2,
  ArrowUpRight,
  ArrowUp,
  X,
} from "lucide-react";
import { invoke } from "./api";
import type {
  Snapshot,
  AppSpec,
  DetectedApp,
  Series,
  Week,
  Preferences,
  LineupEntry,
} from "./types";

type Page = "planner" | "apps" | "learn";
type Modal = {
  title: string;
  value?: string;
  description?: string;
  details?: string;
  confirm?: string;
  danger?: boolean;
  submit?: (value: string) => Promise<void>;
};
type Send = (command: string, args?: unknown) => Promise<void>;
const lessons = [
  {
    title: "iRacing Driving School",
    kind: "FUNDAMENTALS",
    description:
      "Build the foundations: braking, racing lines, weight transfer and car control.",
    link: "Official driving school",
  },
  {
    title: "Danny Lee",
    kind: "BEGINNER COURSE",
    description:
      "A structured course to help you find a consistent rhythm and build good driving habits.",
    link: "Explore the beginner course",
  },
  {
    title: "Input Telemetry Overlay",
    kind: "PRACTICE TOOL",
    description:
      "See your steering, brake and throttle inputs to understand what changes between laps.",
    link: "View the open source project",
  },
];
function useCompact() {
  const [compact, setCompact] = useState(
    () => window.matchMedia("(max-width: 54rem)").matches,
  );
  useEffect(() => {
    const media = window.matchMedia("(max-width: 54rem)");
    const update = () => setCompact(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  return compact;
}
function initialPage(): Page {
  const hash = location.hash.slice(1);
  return hash === "apps" || hash === "learn" ? hash : "planner";
}
function BackToTop({
  top,
  page,
}: {
  top: React.RefObject<HTMLHeadingElement | null>;
  page: Page;
}) {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const heading = top.current;
    setVisible(false);
    if (!heading) return;
    const observer = new IntersectionObserver(([entry]) => {
      setVisible(
        !entry.isIntersecting &&
          entry.boundingClientRect.top < (entry.rootBounds?.top ?? 0),
      );
    });
    observer.observe(heading);
    return () => observer.disconnect();
  }, [top, page]);
  if (!visible) return null;
  return (
    <IconButton
      label="Back to top"
      tooltip="Back to top"
      icon={<Icon icon={ArrowUp} size="md" />}
      variant="primary"
      size="lg"
      elevation="med"
      style={{
        position: "fixed",
        insetInlineEnd:
          "calc(var(--spacing-6) + env(safe-area-inset-right, 0rem))",
        bottom: "calc(var(--spacing-6) + env(safe-area-inset-bottom, 0rem))",
        width: "var(--spacing-12)",
        height: "var(--spacing-12)",
        borderRadius: "var(--radius-full)",
        zIndex: 10,
      }}
      onClick={() => {
        const heading = top.current;
        if (!heading) return;
        heading.focus({ preventScroll: true });
        const behavior = window.matchMedia("(prefers-reduced-motion: reduce)")
          .matches
          ? "instant"
          : "smooth";
        const scrollers = new Set<Element>();
        for (
          let node: Element | null = heading.parentElement;
          node;
          node = node.parentElement
        ) {
          if (node.scrollTop > 0) scrollers.add(node);
        }
        if (document.scrollingElement) scrollers.add(document.scrollingElement);
        for (const scroller of scrollers)
          scroller.scrollTo({ top: 0, behavior });
      }}
    />
  );
}
function App() {
  const pageHeading = useRef<HTMLHeadingElement>(null);
  const [state, setState] = useState<Snapshot | null>(null);
  const [error, setError] = useState("");
  const [page, setPage] = useState<Page>(initialPage);
  const [modal, setModal] = useState<Modal | null>(null);
  const [modalValue, setModalValue] = useState("");
  const [saving, setSaving] = useState(false);
  const [advancedApp, setAdvancedApp] = useState<string | null>(null);
  const send: Send = useCallback(async (command, args = {}) => {
    try {
      const next = await invoke(command, args);
      setState(next);
      setError("");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setError(message);
      throw error;
    }
  }, []);
  const act = (command: string, args?: unknown) =>
    void send(command, args).catch(() => {});
  useEffect(() => {
    void send("state").catch(() => {});
    const update = (event: Event) =>
      setState((event as CustomEvent<Snapshot>).detail);
    const hash = () => setPage(initialPage());
    window.addEventListener("paddock:state", update);
    window.addEventListener("hashchange", hash);
    return () => {
      window.removeEventListener("paddock:state", update);
      window.removeEventListener("hashchange", hash);
    };
  }, [send]);
  const openModal = useCallback((value: Modal) => {
    setModalValue(value.value ?? "");
    setModal(value);
  }, []);
  async function saveModal() {
    if (!modal?.submit) return;
    setSaving(true);
    try {
      await modal.submit(modalValue);
      setModal(null);
    } catch {
    } finally {
      setSaving(false);
    }
  }
  const ready = state?.phase === "Idle";
  const changing = state?.phase === "Starting" || state?.phase === "Stopping";
  const phase = state?.phase ?? "Connecting";
  const phaseLabel =
    phase === "Idle"
      ? "Ready"
      : phase === "CleanupFailed"
        ? "Stop failed"
        : phase;
  useEffect(() => {
    if (!["Starting", "Running"].includes(phase)) return;
    let last = 0;
    const active = () => {
      if (Date.now() - last < 1000) return;
      last = Date.now();
      void invoke("window.activity").catch(() => {});
    };
    document.addEventListener("pointerdown", active, true);
    document.addEventListener("keydown", active, true);
    return () => {
      document.removeEventListener("pointerdown", active, true);
      document.removeEventListener("keydown", active, true);
    };
  }, [phase]);
  const deleteProfile = () =>
    openModal({
      title: "Delete " + state?.profile_name + "?",
      description:
        "This deletes this profile's app list and launch settings. Installed applications, favorites and race plans are kept.",
      danger: true,
      confirm: "Delete profile",
      submit: () => send("profile.delete", { id: state?.active_profile }),
    });
  const profileItems = [
    {
      label: "New profile",
      onClick: () =>
        openModal({
          title: "New profile",
          description:
            "Start with an empty app list, then add the apps you want to manage together.",
          confirm: "Create profile",
          submit: async (name) => send("profile.create", { name }),
        }),
    },
    {
      label: "Duplicate current profile",
      onClick: () =>
        openModal({
          title: "Duplicate profile",
          value: `${state?.profile_name ?? ""} copy`,
          confirm: "Create copy",
          submit: async (name) =>
            send("profile.create", { name, duplicate: true }),
        }),
    },
    {
      label: "Rename profile",
      onClick: () =>
        openModal({
          title: "Rename profile",
          value: state?.profile_name,
          confirm: "Save name",
          submit: async (name) => send("profile.rename", { name }),
        }),
    },
    {
      label: "Delete profile",
      variant: "destructive" as const,
      isDisabled: !ready || (state?.profiles.length ?? 0) <= 1,
      onClick: deleteProfile,
    },
  ];
  return (
    <Theme theme={themes[defaultThemeSlug]} mode={state?.theme ?? "dark"}>
      <AppShell
        contentPadding={0}
        variant="wash"
        sideNav={
          <SideNav
            collapsible
            resizable={{ defaultWidth: 244, minWidth: 200, maxWidth: 340 }}
            header={
              <SideNavHeading
                heading="Paddock"
                headingHref="#planner"
                icon={
                  <Thumbnail src={brandIcon} alt="Paddock" label="Paddock" />
                }
              />
            }
            footer={
              <VStack padding={4} gap={3}>
                <HStack gap={2} vAlign="center">
                  <StatusDot
                    variant={
                      phase === "CleanupFailed"
                        ? "error"
                        : phase === "Running"
                          ? "success"
                          : changing
                            ? "warning"
                            : "neutral"
                    }
                    label={phaseLabel}
                  />
                  <Text type="supporting">{phaseLabel}</Text>
                </HStack>
                <Button
                  label="Hide Paddock"
                  icon={<Icon icon={EyeOff} size="sm" />}
                  width="100%"
                  size="sm"
                  variant="secondary"
                  onClick={() => act("window.hide")}
                />
                <Text type="supporting">
                  {state ? `Paddock ${state.version}` : "Connecting…"}
                </Text>
              </VStack>
            }
          >
            <SideNavSection title="WORKSPACE">
              <SideNavItem
                label="Planner"
                icon={LayoutGrid}
                href="#planner"
                isSelected={page === "planner"}
                onClick={() => setPage("planner")}
              />
              <SideNavItem
                label="Apps"
                icon={Monitor}
                href="#apps"
                isSelected={page === "apps"}
                onClick={() => setPage("apps")}
              />
              <SideNavItem
                label="Learn"
                icon={BookOpen}
                href="#learn"
                isSelected={page === "learn"}
                onClick={() => setPage("learn")}
              />
            </SideNavSection>
          </SideNav>
        }
      >
        <Layout
          height="fill"
          contentWidth="100%"
          content={
            <LayoutContent padding={6}>
              <VStack gap={6}>
                <HStack hAlign="between" vAlign="start" wrap="wrap" gap={4}>
                  <VStack gap={2}>
                    <Text type="supporting">
                      {page === "apps"
                        ? (state?.profile_name ?? "APP PROFILES")
                        : "iRacing Companion"}
                    </Text>
                    <Heading level={1} ref={pageHeading} tabIndex={-1}>
                      {page === "planner"
                        ? "Season planner"
                        : page === "apps"
                          ? "App manager"
                          : "Learn"}
                    </Heading>
                    <Text color="secondary">
                      {page === "planner"
                        ? state?.schedule
                          ? `${state.schedule.season} · ${state.schedule.weeks.length} weeks · ${state.schedule.planned} planned`
                          : "Plan your next race week"
                        : page === "apps"
                          ? "Your apps, ready when you are."
                          : "A little more understanding. A little more consistency."}
                    </Text>
                  </VStack>
                  <HStack gap={2} wrap="wrap">
                    <Button
                      label="Launch all"
                      icon={<Icon icon={Play} size="sm" />}
                      variant="primary"
                      isDisabled={
                        !state || !["Idle", "Running"].includes(phase)
                      }
                      isLoading={phase === "Starting"}
                      onClick={() => act("start")}
                    />
                    <Button
                      label={
                        phase === "CleanupFailed" ? "Retry stop" : "Stop all"
                      }
                      variant="destructive"
                      icon={<Icon icon={Square} size="sm" />}
                      isDisabled={!state || phase === "Stopping"}
                      isLoading={phase === "Stopping"}
                      onClick={() =>
                        act(phase === "CleanupFailed" ? "retry_stop" : "stop")
                      }
                    />
                  </HStack>
                </HStack>
                {error && (
                  <Banner
                    status="error"
                    title="Something needs attention"
                    description={error}
                    isDismissable
                    onDismiss={() => setError("")}
                  />
                )}
                {!state && !error && (
                  <Text color="secondary">Loading your workspace…</Text>
                )}
                {state && page === "planner" && (
                  <Planner state={state} send={send} openModal={openModal} />
                )}
                {state && page === "apps" && (
                  <Apps
                    state={state}
                    send={send}
                    openModal={openModal}
                    onAdvanced={setAdvancedApp}
                    profileControls={
                      <VStack as="section" aria-label="App profiles" gap={3}>
                        <HStack
                          hAlign="between"
                          vAlign="center"
                          wrap="wrap"
                          gap={3}
                        >
                          <VStack gap={1}>
                            <Heading level={2}>Profile</Heading>
                            <Text type="supporting">
                              Choose the apps and launch settings you want to
                              use together.
                            </Text>
                          </VStack>
                          <HStack
                            gap={2}
                            vAlign="center"
                            width="min(24rem, 100%)"
                          >
                            <StackItem size="fill">
                              <ContextMenu
                                items={profileItems}
                                label="Profile actions"
                                isDisabled={!ready}
                                presentation="adaptive"
                              >
                                <Selector
                                  label="Profile"
                                  isLabelHidden
                                  value={state.active_profile}
                                  options={state.profiles.map((profile) => ({
                                    value: profile.id,
                                    label: profile.name,
                                  }))}
                                  onChange={(id) =>
                                    act("profile.select", { id })
                                  }
                                  isDisabled={!ready}
                                  disabledMessage="Stop the current session to switch profiles."
                                  width="100%"
                                  variant="secondary"
                                />
                              </ContextMenu>
                            </StackItem>
                            <MoreMenu
                              label="Manage profiles"
                              isDisabled={!ready}
                              items={profileItems}
                            />
                          </HStack>
                        </HStack>
                        <Divider />
                      </VStack>
                    }
                  />
                )}
                {state && page === "learn" && (
                  <VStack gap={6}>
                    {lessons.map((lesson, index) => (
                      <React.Fragment key={lesson.title}>
                        <HStack
                          wrap="wrap"
                          hAlign="between"
                          vAlign="center"
                          gap={4}
                        >
                          <VStack gap={2} maxWidth="42rem">
                            <Text type="supporting">{lesson.kind}</Text>
                            <Heading level={2}>{lesson.title}</Heading>
                            <Text color="secondary">{lesson.description}</Text>
                          </VStack>
                          <Button
                            label={lesson.link}
                            variant="secondary"
                            endContent={<Icon icon={ArrowUpRight} size="sm" />}
                            onClick={() => act("learn.open", { index })}
                          />
                        </HStack>
                        {index < lessons.length - 1 && <Divider />}
                      </React.Fragment>
                    ))}
                  </VStack>
                )}
                {state?.message && (
                  <Text type="supporting" role="status">
                    {state.message}
                  </Text>
                )}
              </VStack>
            </LayoutContent>
          }
        />
      </AppShell>
      <AppAdvanced
        app={state?.apps.find((app) => app.id === advancedApp) ?? null}
        editable={ready}
        close={() => setAdvancedApp(null)}
        send={send}
      />
      {!modal && !advancedApp && <BackToTop top={pageHeading} page={page} />}
      <Dialog
        isOpen={!!modal}
        onOpenChange={(open) => {
          if (!open && !saving) setModal(null);
        }}
        purpose={modal?.submit ? "form" : "info"}
        width="min(38rem, 100%)"
        maxHeight="85dvh"
      >
        <DialogHeader title={modal?.title ?? ""} />
        <VStack gap={4} padding={5}>
          {modal?.description && (
            <Text color="secondary">{modal.description}</Text>
          )}
          {modal?.submit && !modal.danger && (
            <TextInput
              label="Name"
              value={modalValue}
              onChange={setModalValue}
              hasAutoFocus
              onEnter={() => void saveModal()}
              width="100%"
            />
          )}
          {modal?.details && (
            <VStack gap={1}>
              {modal.details.split("\n").map((line, index) => (
                <Text as="p" key={index}>
                  {line || "\u00a0"}
                </Text>
              ))}
            </VStack>
          )}
          <HStack gap={2} hAlign="end" wrap="wrap">
            <Button
              label={modal?.submit ? "Cancel" : "Close"}
              variant="secondary"
              isDisabled={saving}
              onClick={() => setModal(null)}
            />
            {modal?.submit && (
              <Button
                label={modal.confirm ?? "Save"}
                variant={modal.danger ? "destructive" : "primary"}
                isLoading={saving}
                isDisabled={!modal.danger && !modalValue.trim()}
                onClick={() => void saveModal()}
              />
            )}
          </HStack>
        </VStack>
      </Dialog>
    </Theme>
  );
}

function AppAdvanced({
  app,
  editable,
  close,
  send,
}: {
  app: AppSpec | null;
  editable: boolean;
  close: () => void;
  send: Send;
}) {
  return (
    <Dialog
      isOpen={!!app}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      purpose="info"
      width="min(35rem, 100%)"
    >
      <DialogHeader title={app ? app.name + " · Advanced" : "Advanced"} />
      <VStack gap={4} padding={5}>
        <Switch
          label="Hide management window after game exit"
          value={app?.rehide_after_game ?? false}
          isDisabled={!editable}
          onChange={(value) => {
            if (app)
              void send("app.update", {
                index: app.index,
                rehide_after_game: value,
              }).catch(() => {});
          }}
        />
        <Text color="secondary">
          For apps such as Edge Overlays that reopen their management window
          after iRacing closes. In Background mode, hide a reopened management
          window once within 15 seconds of game exit. A window opened manually
          during this interval may also be hidden.
        </Text>
        <Text type="supporting">
          Off by default. Normal startup hiding runs once; later reopened
          windows stay open. Trading Paints automatically waits two seconds for
          startup initialization. Opening it through Paddock cancels pending
          startup hiding.
        </Text>
        {!app?.tray && (
          <Text type="supporting">
            Choose Background under On launch to use game-exit hiding.
          </Text>
        )}
        <HStack hAlign="end">
          <Button label="Close" onClick={close} />
        </HStack>
      </VStack>
    </Dialog>
  );
}
function Apps({
  state,
  send,
  openModal,
  profileControls,
  onAdvanced,
}: {
  onAdvanced: (id: string) => void;
  profileControls: React.ReactNode;
  state: Snapshot;
  send: Send;
  openModal: (modal: Modal) => void;
}) {
  const compact = useCompact();
  const editable = state.phase === "Idle";
  const act = (command: string, args?: unknown) =>
    void send(command, args).catch(() => {});
  const count = state.apps.filter((app) => app.enabled).length;
  const running = state.apps.filter((app) => app.running > 0).length;
  function menu(app: AppSpec) {
    return [
      {
        label: "Open window",
        isDisabled: !app.running,
        onClick: () => act("app.restore", { index: app.index }),
      },
      {
        label: "Hide window",
        isDisabled: !app.running,
        onClick: () => act("app.hide", { index: app.index }),
      },
      {
        label: "Rename app",
        isDisabled: !editable,
        onClick: () =>
          openModal({
            title: "Rename app",
            value: app.name,
            submit: (name) => send("app.update", { index: app.index, name }),
          }),
      },
      {
        label: "Change executable",
        isDisabled: !editable,
        onClick: () => act("app.browse", { index: app.index }),
      },
      { label: "Advanced", onClick: () => onAdvanced(app.id) },
      {
        label: "App details",
        onClick: () =>
          openModal({
            title: app.name,
            details: `${app.path}\n\n${app.running ? "Running" : "Stopped"} · ${app.tray ? "Hidden after startup" : "Visible after startup"}\n${app.running ? app.hide_status : ""}`,
          }),
      },
      {
        label: "Remove from profile",
        isDisabled: !editable,
        onClick: () =>
          openModal({
            title: `Remove ${app.name}?`,
            description:
              "This removes the app from this profile. The application stays installed.",
            danger: true,
            confirm: "Remove app",
            submit: () => send("app.remove", { index: app.index }),
          }),
      },
    ];
  }
  const columns: TableColumn<AppSpec & Record<string, unknown>>[] = [
    {
      key: "name",
      header: "Application",
      width: proportional(3, { minWidth: compact ? 140 : 220 }),
      renderCell: (app) => (
        <VStack gap={1}>
          <Text weight="semibold">{app.name}</Text>
          {compact ? (
            <Text type="supporting">
              {app.running
                ? "Running"
                : !app.exists
                  ? "Check executable"
                  : "Stopped"}
            </Text>
          ) : (
            <Text type="supporting" maxLines={1}>
              {app.path}
            </Text>
          )}
        </VStack>
      ),
    },
    ...(!compact
      ? [
          {
            key: "running",
            header: "Status",
            width: proportional(1, { minWidth: 105 }),
            renderCell: (app: AppSpec) => (
              <HStack gap={2} vAlign="center">
                <StatusDot
                  variant={
                    app.running
                      ? "success"
                      : !app.exists
                        ? "warning"
                        : "neutral"
                  }
                  label={
                    app.running
                      ? "Running"
                      : !app.exists
                        ? "Check executable"
                        : "Stopped"
                  }
                />
                <Text type="supporting">
                  {app.running
                    ? "Running"
                    : !app.exists
                      ? "Check path"
                      : "Stopped"}
                </Text>
              </HStack>
            ),
          },
        ]
      : []),
    {
      key: "enabled",
      header: "Include",
      width: pixel(80),
      renderCell: (app) => (
        <Switch
          label={`Include ${app.name}`}
          isLabelHidden
          size="sm"
          value={app.enabled}
          isDisabled={!editable}
          onChange={(enabled) =>
            act("app.update", { index: app.index, enabled })
          }
        />
      ),
    },
    ...(!compact
      ? [
          {
            key: "tray",
            header: "On launch",
            width: proportional(1, { minWidth: 100 }),
            renderCell: (app: AppSpec) => (
              <Selector
                label={`${app.name} startup`}
                isLabelHidden
                variant="secondary"
                size="sm"
                options={[
                  { value: "window", label: "Window" },
                  { value: "tray", label: "Background" },
                ]}
                value={app.tray ? "tray" : "window"}
                isDisabled={!editable}
                onChange={(value) =>
                  act("app.update", {
                    index: app.index,
                    tray: value === "tray",
                  })
                }
              />
            ),
          },
        ]
      : []),
    {
      key: "actions",
      header: "",
      width: pixel(72),
      renderCell: (app) => (
        <MoreMenu
          label={`Options for ${app.name}`}
          items={
            compact
              ? [
                  ...menu(app).slice(0, 2),
                  {
                    label: app.tray
                      ? "Show window on launch"
                      : "Hide window after startup",
                    isDisabled: !editable,
                    onClick: () =>
                      act("app.update", { index: app.index, tray: !app.tray }),
                  },
                  ...menu(app).slice(2),
                ]
              : menu(app)
          }
          alignment="end"
          presentation="adaptive"
        />
      ),
    },
  ];
  return (
    <VStack gap={5}>
      {profileControls}
      <HStack hAlign="between" vAlign="center" wrap="wrap" gap={3}>
        <HStack gap={5} vAlign="center" wrap="wrap">
          <Text type="supporting">
            {count} included · {running} running
          </Text>
          <Switch
            label="Auto-hide Paddock"
            value={state.auto_hide}
            size="sm"
            onChange={(value) => act("auto_hide", { value })}
          />
        </HStack>
        <HStack gap={2} wrap="wrap">
          <Button
            label="Hide all windows"
            size="sm"
            variant="secondary"
            onClick={() => act("hide_helpers")}
            isDisabled={
              !state.apps.some((app) => app.enabled && app.running > 0)
            }
          />
          <Button
            label="Show all windows"
            size="sm"
            variant="secondary"
            onClick={() => act("show_windows")}
            isDisabled={
              !state.apps.some((app) => app.enabled && app.running > 0)
            }
          />
          <Button
            label="Browse executable"
            size="sm"
            icon={<Icon icon={Plus} size="sm" />}
            isDisabled={!editable}
            onClick={() => act("app.add")}
          />
        </HStack>
      </HStack>
      {!editable && (
        <Text type="supporting">
          Stop the current session to edit apps or switch profiles.
        </Text>
      )}
      {state.apps.length ? (
        <Table
          data={state.apps}
          columns={columns}
          idKey="id"
          density="spacious"
          hasHover
          textOverflow="wrap"
        />
      ) : (
        <VStack gap={3} padding={6}>
          <Heading level={2}>Build this game's app list</Heading>
          <Text color="secondary">
            Add the game launcher and any companion apps you want to start
            together.
          </Text>
          <HStack>
            <Button
              label="Add your first app"
              icon={<Icon icon={Plus} size="sm" />}
              onClick={() => act("app.add")}
            />
          </HStack>
        </VStack>
      )}
      <DetectedApps
        apps={state.detected_apps ?? []}
        editable={editable}
        act={act}
      />
      <Divider />
      <HStack gap={4} hAlign="between" wrap="wrap">
        <Selector
          label="Appearance"
          variant="secondary"
          value={state.theme}
          options={[
            { value: "system", label: "System" },
            { value: "dark", label: "Dark" },
            { value: "light", label: "Light" },
          ]}
          onChange={(value) => act("theme", { value })}
        />
        <Button
          label="Data folder"
          variant="secondary"
          onClick={() => act("data.folder")}
        />
      </HStack>
      <Text type="supporting">
        Apps stay open when the game exits. Use Stop all to end the session.
      </Text>
    </VStack>
  );
}

function DetectedApps({
  apps,
  editable,
  act,
}: {
  apps: DetectedApp[];
  editable: boolean;
  act: (command: string, args?: unknown) => void;
}) {
  const columns: TableColumn<DetectedApp>[] = [
    {
      key: "name",
      header: "Application",
      width: proportional(1, { minWidth: 180 }),
      renderCell: (app) => (
        <VStack gap={1}>
          <Text weight="semibold">{app.name}</Text>
          <Text type="supporting" maxLines={2}>
            {app.path}
          </Text>
        </VStack>
      ),
    },
    {
      key: "action",
      header: "",
      width: pixel(112),
      renderCell: (app) =>
        app.added ? (
          <Text type="supporting">Added</Text>
        ) : (
          <Button
            label={"Add " + app.name}
            size="sm"
            isDisabled={!editable}
            onClick={() => act("app.add_detected", { id: app.id })}
          >
            Add
          </Button>
        ),
    },
  ];
  return (
    <VStack as="section" aria-label="Detected apps" gap={3}>
      <HStack hAlign="between" vAlign="center" wrap="wrap" gap={3}>
        <VStack gap={1}>
          <Heading level={2}>Detected apps</Heading>
          <Text type="supporting">
            Known apps found at default install locations. Choose which ones to
            add to this profile.
          </Text>
        </VStack>
        <Button
          label="Scan again"
          size="sm"
          icon={<Icon icon={RotateCcw} size="sm" />}
          onClick={() => act("app.scan")}
        />
      </HStack>
      {apps.length ? (
        <LayoutContent padding={0} isScrollable={false}>
          <Table
            aria-label="Detected applications"
            data={apps}
            columns={columns}
            idKey="id"
            density="balanced"
            dividers="rows"
            textOverflow="wrap"
          />
        </LayoutContent>
      ) : (
        <Text color="secondary">
          No supported apps found at default locations. Use Browse executable to
          select an installed app.
        </Text>
      )}
    </VStack>
  );
}
function localTime(value: string) {
  const date = new Date(
    /^\d{4}-\d{2}-\d{2}$/.test(value) ? value + "T00:00:00Z" : value,
  );
  if (!Number.isFinite(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    weekday: "short",
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    timeZoneName: "short",
  }).format(date);
}
function weekTiming(weeks: { number: number; start: string }[], week: number) {
  const index = weeks.findIndex((item) => item.number === week);
  if (index < 0) return "";
  const start = weeks[index].start;
  const next =
    weeks[index + 1]?.start ??
    new Date(Date.parse(start + "T00:00:00Z") + 7 * 86400000).toISOString();
  return "Starts " + localTime(start) + " · Next reset " + localTime(next);
}
type SeriesJump = { id: string; token: number };
function seriesAnchor(id: string) {
  return "planner-series-" + encodeURIComponent(id);
}
function Planner({
  state,
  send,
  openModal,
}: {
  state: Snapshot;
  send: Send;
  openModal: (modal: Modal) => void;
}) {
  const schedule = state.schedule;
  const p = state.preferences;
  const [query, setQuery] = useState(p.query);
  const filterKey = JSON.stringify(p);
  const [jumpTarget, setJumpTarget] = useState<SeriesJump | null>(null);
  const jumpSequence = useRef(0);
  const finishJump = useCallback(() => setJumpTarget(null), []);
  const jumpToSeries = useCallback(
    async (id: string) => {
      const token = ++jumpSequence.current;
      try {
        if (!schedule?.series.some((series) => series.id === id)) {
          setQuery("");
          await send("planner.preferences", {
            ...p,
            query: "",
            category: null,
            categories: [],
            classes: [],
            favorites_only: false,
          });
        }
        if (token === jumpSequence.current) setJumpTarget({ id, token });
      } catch {
        // The normal command error banner reports a failed filter update.
      }
    },
    [schedule?.series, p, send],
  );
  const act = useCallback(
    (command: string, args?: unknown) =>
      void send(command, args).catch(() => {}),
    [send],
  );
  useEffect(() => {
    const handle = setTimeout(() => {
      if (query !== p.query) act("planner.preferences", { ...p, query });
    }, 220);
    return () => clearTimeout(handle);
  }, [query, p.query]);
  function preferences(patch: Partial<Preferences>) {
    act("planner.preferences", { ...p, query, ...patch });
  }
  function toggle(key: "classes" | "categories", value: string) {
    const current = p[key];
    preferences({
      [key]: current.includes(value)
        ? current.filter((item) => item !== value)
        : [...current, value],
      ...(key === "categories" ? { category: null } : {}),
    });
  }
  if (!schedule)
    return (
      <VStack gap={4}>
        <Heading level={2}>Your season starts here</Heading>
        <Text color="secondary">
          Refresh the official iRacing schedule to load race weeks and series.
        </Text>
        <HStack>
          <Button
            label="Refresh schedule"
            isLoading={state.refreshing}
            onClick={() => act("refresh")}
          />
        </HStack>
      </VStack>
    );
  const currentWeek = schedule.current_week;
  return (
    <VStack gap={5}>
      <HStack hAlign="between" wrap="wrap" gap={3}>
        <VStack gap={1}>
          <Text weight="semibold">
            {currentWeek
              ? `Current week ${currentWeek} of ${schedule.weeks.length}`
              : "Season overview"}
          </Text>
          <Text type="supporting">
            {currentWeek
              ? weekTiming(schedule.weeks, currentWeek)
              : "No active race week today. All season plans remain available."}
          </Text>
        </VStack>
        <HStack gap={2} wrap="wrap">
          <Button
            label="Official PDF"
            variant="secondary"
            size="sm"
            onClick={() => act("schedule.source")}
          />
          <Button
            label="Refresh schedule"
            size="sm"
            isLoading={state.refreshing}
            onClick={() => act("refresh")}
          />
        </HStack>
      </HStack>
      <WeeklyLineup
        entries={schedule.lineup ?? []}
        onJump={jumpToSeries}
        week={currentWeek}
        act={act}
      />
      <VStack gap={3}>
        <HStack gap={2} wrap="wrap">
          {state.categories.map((category) => (
            <Button
              key={category.value}
              label={category.label}
              size="sm"
              variant={
                p.categories.includes(category.value) ||
                p.category === category.value
                  ? "primary"
                  : "secondary"
              }
              onClick={() => toggle("categories", category.value)}
            />
          ))}
        </HStack>
        <HStack hAlign="between" gap={3} wrap="wrap">
          <HStack gap={1} wrap="wrap">
            {state.classes.map((license) => (
              <Button
                key={license.value}
                label={license.label}
                tooltip={`License ${license.label}`}
                width={license.value === "Pro" ? 56 : 44}
                size="sm"
                variant={
                  p.classes.includes(license.value) ? "primary" : "secondary"
                }
                onClick={() => toggle("classes", license.value)}
              />
            ))}
            <Button
              label={`Favorites ${schedule.favorite_count}`}
              icon={<Icon icon={Star} size="sm" />}
              variant={p.favorites_only ? "primary" : "secondary"}
              size="sm"
              onClick={() => preferences({ favorites_only: !p.favorites_only })}
            />
            <Button
              label="Reset"
              size="sm"
              variant="secondary"
              onClick={() => {
                setQuery("");
                act("planner.reset");
              }}
            />
          </HStack>
          <Selector
            label="Sort series"
            isLabelHidden
            variant="secondary"
            size="sm"
            value={p.sort}
            options={[
              { value: "Class", label: "License" },
              { value: "Name", label: "Name" },
              { value: "Category", label: "Discipline" },
            ]}
            onChange={(sort) => preferences({ sort })}
          />
        </HStack>
        <TextInput
          label="Search series, cars or tracks"
          isLabelHidden
          placeholder="Search series, cars or tracks"
          type="search"
          value={query}
          onChange={setQuery}
          startIcon={Search}
          hasClear
          width="100%"
        />
      </VStack>
      <HStack hAlign="between" gap={3} wrap="wrap">
        <Text type="supporting">
          {schedule.series.length} series · Select the race weeks you want to
          drive
        </Text>
        {currentWeek !== null && schedule.can_undo.includes(currentWeek) && (
          <Button
            label={`Undo week ${currentWeek}`}
            variant="secondary"
            size="sm"
            icon={<Icon icon={RotateCcw} size="sm" />}
            onClick={() => act("planner.undo", { week: currentWeek })}
          />
        )}
      </HStack>
      {!schedule.series.length && (
        <VStack padding={5} gap={2}>
          <Heading level={2}>No matching series</Heading>
          <Text color="secondary">
            Try another search or reset the filters.
          </Text>
        </VStack>
      )}
      <ContinuousSeries
        series={schedule.series}
        week={currentWeek}
        categories={state.categories}
        act={act}
        openModal={openModal}
        filterKey={filterKey}
        jumpTarget={jumpTarget}
        onJumpComplete={finishJump}
      />
    </VStack>
  );
}

const WeeklyLineup = React.memo(function WeeklyLineup({
  onJump,
  entries,
  week,
  act,
}: {
  onJump: (id: string) => void;
  entries: LineupEntry[];
  week: number | null;
  act: (command: string, args?: unknown) => void;
}) {
  const [isOpen, setIsOpen] = useState(() => {
    try {
      return (
        localStorage.getItem("paddock.planner.lineup.expanded") !== "false"
      );
    } catch {
      return true;
    }
  });
  const onOpenChange = (open: boolean) => {
    setIsOpen(open);
    try {
      localStorage.setItem("paddock.planner.lineup.expanded", String(open));
    } catch {
      // Keep the disclosure usable if browser storage is unavailable.
    }
  };
  const columns: TableColumn<LineupEntry>[] = [
    {
      key: "series",
      header: "Series",
      width: proportional(1, { minWidth: 180 }),
      renderCell: (row) => (
        <VStack gap={1}>
          <Link
            href={"#" + seriesAnchor(row.id)}
            weight="semibold"
            tooltip="Go to series schedule"
            onClick={(event) => {
              event.preventDefault();
              onJump(row.id);
            }}
          >
            {row.name}
          </Link>
          <Text type="supporting">
            License {row.class === "Rookie" ? "R" : row.class}
          </Text>
        </VStack>
      ),
    },
    {
      key: "track",
      header: "Circuit",
      width: proportional(1, { minWidth: 160 }),
      renderCell: (row) => <Text>{row.track}</Text>,
    },
    {
      key: "vehicles",
      header: "Car",
      width: proportional(1, { minWidth: 130 }),
      renderCell: (row) => <Text color="secondary">{row.vehicles}</Text>,
    },
    {
      key: "remove",
      header: "",
      width: pixel(48),
      renderCell: (row) => (
        <Button
          label={"Remove " + row.name + " from this week's lineup"}
          tooltip="Remove from this week"
          isIconOnly
          size="sm"
          variant="ghost"
          icon={<Icon icon={X} size="sm" />}
          onClick={() => {
            if (week !== null) act("planner.select", { id: row.id, week });
          }}
        />
      ),
    },
  ];
  return (
    <VStack as="section" aria-label="This week's lineup" gap={3}>
      <Collapsible
        isOpen={isOpen}
        onOpenChange={onOpenChange}
        trigger={
          <HStack hAlign="between" vAlign="center" wrap="wrap" gap={2}>
            <Text type="large" weight="semibold">
              This week's lineup
            </Text>
            <Text type="supporting">
              {week !== null
                ? `Week ${week} · ${entries.length} series`
                : "No active race week"}
            </Text>
          </HStack>
        }
      >
        <LayoutContent padding={0} isScrollable={false}>
          {entries.length ? (
            <Table
              aria-label="Weekly lineup"
              data={entries}
              columns={columns}
              idKey="id"
              density="balanced"
              dividers="rows"
              textOverflow="wrap"
              hasHover
            />
          ) : (
            <Text color="secondary">
              {week !== null
                ? `Check Plan on the highlighted Week ${week} rows to add a series here.`
                : "Your weekly lineup will appear when a race week is active."}
            </Text>
          )}
        </LayoutContent>
      </Collapsible>
      <Divider />
    </VStack>
  );
});
const ContinuousSeries = React.memo(function ContinuousSeries({
  series,
  week,
  categories,
  act,
  openModal,
  filterKey,
  jumpTarget,
  onJumpComplete,
}: {
  series: Series[];
  week: number | null;
  categories: Snapshot["categories"];
  act: (command: string, args?: unknown) => void;
  openModal: (modal: Modal) => void;
  filterKey: string;
  jumpTarget: SeriesJump | null;
  onJumpComplete: () => void;
}) {
  const [visible, setVisible] = useState({ key: filterKey, count: 12 });
  const count = Math.min(
    visible.key === filterKey ? visible.count : 12,
    series.length,
  );
  const end = useRef<HTMLElement>(null);
  const targetIndex = jumpTarget
    ? series.findIndex((item) => item.id === jumpTarget.id)
    : -1;
  useEffect(() => {
    if (!jumpTarget || targetIndex < 0) return;
    if (targetIndex >= count) {
      setVisible({ key: filterKey, count: targetIndex + 1 });
      return;
    }
    const frame = requestAnimationFrame(() => {
      const target = document.getElementById(seriesAnchor(jumpTarget.id));
      if (!target) return;
      target.focus({ preventScroll: true });
      target.scrollIntoView({
        block: "start",
        behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches
          ? "instant"
          : "smooth",
      });
      onJumpComplete();
    });
    return () => cancelAnimationFrame(frame);
  }, [jumpTarget, targetIndex, count, filterKey, onJumpComplete]);
  useEffect(() => {
    const element = end.current;
    if (!element || count >= series.length) return;
    if (typeof IntersectionObserver === "undefined") {
      setVisible({ key: filterKey, count: series.length });
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting))
          setVisible((previous) => ({
            key: filterKey,
            count: Math.min(
              series.length,
              (previous.key === filterKey ? previous.count : 12) + 12,
            ),
          }));
      },
      { rootMargin: "600px" },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [count, series.length, filterKey]);
  return (
    <VStack as="section" aria-label="Season series" gap={3}>
      <Grid
        columns={{ minWidth: 350, max: 3, repeat: "fit" }}
        gap={5}
        align="start"
      >
        {series.slice(0, count).map((item) => (
          <SeriesTable
            key={item.id}
            series={item}
            week={week}
            categories={categories}
            act={act}
            openModal={openModal}
          />
        ))}
      </Grid>
      <HStack
        ref={end}
        id="planner-series-end"
        aria-hidden="true"
        paddingBlock={1}
      >
        <Divider />
      </HStack>
    </VStack>
  );
});

const SeriesTable = React.memo(
  function SeriesTable({
    series,
    week,
    categories,
    act,
    openModal,
  }: {
    series: Series;
    week: number | null;
    categories: Snapshot["categories"];
    act: (command: string, args?: unknown) => void;
    openModal: (modal: Modal) => void;
  }) {
    const columns: TableColumn<Week>[] = [
      {
        key: "number",
        header: "Wk",
        width: pixel(56),
        renderCell: (item) => (
          <Text
            color={item.number === week ? "accent" : "secondary"}
            weight={item.number === week ? "bold" : "normal"}
          >
            {`${item.number}`.padStart(2, "0")}
          </Text>
        ),
      },
      {
        key: "track",
        header: "Circuit",
        width: proportional(1, { minWidth: 120 }),
        renderCell: (item) => (
          <Text
            color={item.number === week ? "accent" : "secondary"}
            weight={item.number === week ? "semibold" : "normal"}
          >
            {item.track ?? "No race"}
          </Text>
        ),
      },
      {
        key: "selected",
        header: "Plan",
        width: pixel(64),
        renderCell: (item) =>
          item.track ? (
            <CheckboxInput
              label={`${series.name} · Week ${item.number} · ${item.track}`}
              isLabelHidden
              value={item.selected}
              size="sm"
              onChange={() =>
                act("planner.select", { id: series.id, week: item.number })
              }
            />
          ) : null,
      },
    ];
    return (
      <Card padding={4} id={seriesAnchor(series.id)} tabIndex={-1}>
        <VStack gap={3}>
          <HStack hAlign="between" gap={2} vAlign="start">
            <StackItem size="fill">
              <VStack gap={1}>
                <Text type="supporting">
                  {categories.find((item) => item.value === series.category)
                    ?.label ?? series.category}{" "}
                  · License {series.class === "Rookie" ? "R" : series.class}
                </Text>
                <Heading level={3} accessibilityLevel={2}>
                  {series.name}
                </Heading>
              </VStack>
            </StackItem>
            <HStack gap={0}>
              <Button
                label={
                  series.favorite
                    ? `Unfavorite ${series.name}`
                    : `Favorite ${series.name}`
                }
                tooltip={series.favorite ? "Remove favorite" : "Add favorite"}
                isIconOnly
                icon={<Icon icon={Star} size="sm" />}
                variant={series.favorite ? "primary" : "ghost"}
                size="sm"
                onClick={() => act("planner.favorite", { id: series.id })}
              />
              <MoreMenu
                label={`Options for ${series.name}`}
                size="sm"
                items={[
                  {
                    label: "Series details",
                    onClick: () =>
                      openModal({
                        title: series.name,
                        details: `${series.vehicles}\n\n${series.times}\n\n${series.conditions}`,
                      }),
                  },
                ]}
              />
            </HStack>
          </HStack>
          <Table
            data={series.weeks}
            columns={columns}
            idKey="number"
            density="compact"
            hasHover
            dividers="rows"
            textOverflow="wrap"
          />
        </VStack>
      </Card>
    );
  },
  (previous, next) =>
    previous.week === next.week &&
    previous.act === next.act &&
    previous.openModal === next.openModal &&
    JSON.stringify(previous.series) === JSON.stringify(next.series) &&
    previous.categories.find((item) => item.value === previous.series.category)
      ?.label ===
      next.categories.find((item) => item.value === next.series.category)
        ?.label,
);

createRoot(document.getElementById("root")!).render(<App />);
