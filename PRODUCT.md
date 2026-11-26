# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

One person: the owner of a single home Linux machine (Arch) that runs a media server (Plex), file sharing, sync, dev tooling and assorted self-hosted services. They are a backend and infrastructure engineer, fluent in systemd and Linux, and use the dashboard to keep that machine healthy.

Usual job: a quick health glance (is the machine fine, is anything broken) in a few seconds. The dashboard must still carry the full power for the rarer jobs: fixing a broken service (find it, read its logs, restart, stop, enable or disable it) and hunting a resource hog (which process or service is eating CPU or memory).

## Product Purpose

A central place to see how the machine is doing and manage its services, without SSH. Success: one look answers "is everything fine?", and when it isn't, the culprit and the fix are a tap or two away.

## Operating Context

- Opened from an iPhone (added to the home screen) and a laptop browser, always over WireGuard; the server listens only on the VPN address.
- Three scenes: phone at night in a dim room, phone out and about in daylight for short sessions, laptop at a desk during the day for longer sessions next to other work.
- Data refreshes live every 2 seconds (stats, processes) and 5 seconds (services).

## Capabilities and Constraints

- Live CPU (overall, per thread, load, temperature), memory and swap with two-minute history, disks, network rates, and the battery (charge, time left or to full, power draw, health) when there is one.
- Top processes by CPU or memory; the owner's own processes can be sent SIGTERM.
- Every system and user systemd service: state, memory, CPU time, restart count, unit file, last 200 journal lines; start, stop, restart, enable, disable. Failed and crash-looping services are surfaced first.
- Stopping units that keep the machine reachable (sshd, NetworkManager, wg-quick, dbus, systemd-*) warns before acting.
- No login; access control is the WireGuard network. System-service control depends on a polkit rule.
- Built as a Rust (Axum) server rendering HTML for htmx with idiomorph; all assets compiled into one binary. No build step for CSS/JS; no external CDNs at runtime.

## Brand Commitments

Name: sysdash. No other brand assets exist.

The owner's binding visual choice (2026-11-15): one white theme on every device (no dark mode), with soft pastel colours; strong saturated fields and large blue areas were rejected as overwhelming.

The owner's explicit constraints for its look: not corporate (no generic SaaS admin template look), not busy or gamer-ish (no neon, glow, sci-fi HUD), not sparse (no big empty areas), and easy to read on a phone. Earlier versions were rejected as "ugly", then as "pale and not elegant".

## Evidence on Hand

All content is the machine's live data. No screenshots, testimonials or marketing copy exist or are needed.

## Product Principles

1. The healthy state reads in one glance; problems interrupt it.
2. Power on demand: every action and detail is reachable, but none crowds the glance.
3. Phone first, laptop equal: nothing works only with a mouse.
4. Truthful numbers: live values, units and states exactly as the system reports them.
5. Dangerous actions confirm; actions that could cut remote access say so.
