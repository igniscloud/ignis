SHELL := /bin/sh

.PHONY: sync-skills sync-skill-references

sync-skills: sync-skill-references

sync-skill-references:
	./scripts/sync_skill_references.sh
