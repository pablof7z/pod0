#!/usr/bin/env ruby
# frozen_string_literal: true

require "json"
require "open3"

event_path = ENV.fetch("GITHUB_EVENT_PATH")
payload = JSON.parse(File.read(event_path))
pull_request = payload["pull_request"]
exit 0 unless pull_request

body = pull_request["body"].to_s
required_sections = {
  "tldr" => "## TLDR",
  "overview" => "## Overview",
  "validation" => "## Validation",
  "decisions / tradeoffs" => "## Decisions / Tradeoffs"
}

sections = Hash.new { |hash, key| hash[key] = [] }
current = nil

body.each_line do |line|
  if (match = line.match(/^##\s+(.+?)\s*$/))
    current = match[1].strip.downcase
    next
  end
  next unless current

  cleaned = line.gsub(/<!--.*?-->/, "").strip
  next if cleaned.empty?
  next if ["-", "*", "- [ ]", "* [ ]"].include?(cleaned)

  sections[current] << cleaned
end

missing = required_sections.select do |key, _heading|
  sections[key].empty?
end.values

unless missing.empty?
  warn "::error::PR description is missing required non-empty section(s): #{missing.join(", ")}"
  warn "Use .github/pull_request_template.md and include exact validation commands or 'Not run: <reason>'."
  exit 1
end

def changed_files(base_sha)
  stdout, stderr, status = Open3.capture3("git", "diff", "--name-only", "#{base_sha}...HEAD")
  unless status.success?
    warn "::error::Unable to list changed files for validation scope: #{stderr.strip}"
    exit 1
  end
  stdout.lines.map(&:strip).reject(&:empty?)
end

def rust_file?(path)
  return true if path.end_with?(".rs")
  return true if path == "Cargo.toml" || path == "Cargo.lock"

  path.end_with?("/Cargo.toml", "/Cargo.lock", "/build.rs")
end

def swift_ios_file?(path)
  return true if path.start_with?("App/", "AppTests/", "AppUITests/")
  return true if path.start_with?("Podcastr.xcodeproj/", "Podcastr.xcworkspace/")

  ["Project.swift", "Tuist.swift"].include?(path)
end

def android_file?(path)
  path.start_with?("android/")
end

base_sha = pull_request.dig("base", "sha").to_s
if base_sha.empty?
  warn "::error::Missing pull_request.base.sha; cannot determine touched validation scope."
  exit 1
end

files = changed_files(base_sha)
validation = sections["validation"].join("\n").downcase

families = []
if files.any? { |path| rust_file?(path) }
  families << ["Rust", /cargo\s+(test|check|build|clippy)|not run:.*cargo|rust workspace build gate/m]
end
if files.any? { |path| swift_ios_file?(path) }
  families << ["Swift/iOS", /xcodebuild|not run:.*(xcodebuild|swift|ios)|build and test/m]
end
if files.any? { |path| android_file?(path) }
  families << ["Android", /gradle|gradlew|android kotlin|not run:.*(android|gradle|kotlin)/m]
end

missing_families = families.reject { |_label, pattern| validation.match?(pattern) }.map(&:first)

unless missing_families.empty?
  warn "::error::Validation section does not cover touched code family/families: #{missing_families.join(", ")}"
  warn "List the focused test command(s), relevant CI gate(s), or 'Not run: <reason>' for each touched family."
  exit 1
end

puts "PR description contract: clean."
