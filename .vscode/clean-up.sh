#!/bin/bash

pkill -TERM --parent "$(pgrep '^xinit')" || true
