# Makefile for pagemap

CC = gcc
CFLAGS = -std=gnu11

.PHONY: all
all: pagemap pagemap2 pagetest attacktest

pagemap: pagemap.c
	$(CC) $(CFLAGS) $^ -o $@
pagemap2: pagemap2.c
	$(CC) $(CFLAGS) $^ -o $@
pagetest: pagetest.c
	$(CC) $(CFLAGS) $^ -o $@
attacktest: attacktest.c
	$(CC) $(CFLAGS) $^ -o $@

.PHONY: clean
clean:
	-rm pagemap pagemap2 pagetest attacktest
