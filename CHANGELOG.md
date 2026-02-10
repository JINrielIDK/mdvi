# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- Vim-style full-page navigation with `Ctrl-f` (down) and `Ctrl-b` (up).
- Visual highlighting for search matches in the document view, with stronger emphasis on the active match.

### Changed
- Renamed the CLI and package from `mdview` to `mdvi`.
- Updated in-app title/help text and README command examples to use `mdvi`.
- Search matches are rebuilt after file reload so navigation and highlights stay accurate.
