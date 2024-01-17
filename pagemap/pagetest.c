// A simple program to test the pageattack procedure. It allocates a number of
// pages and then prints each PFN using the pagemap program. The output should
// contain the desired PFN in the correct spot.

#define PAGE_SIZE 4096

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char *argv[])
{
    int npages = argv[1] ? atoi(argv[1]) : 100;

    int pages[npages * PAGE_SIZE / sizeof(int)];
    memset(pages, 0, npages * PAGE_SIZE);

    for (int i = 0; i < npages; i++)
    {
        int idx = i * PAGE_SIZE / sizeof(int);
        pages[idx] = i;
    }

    int *first_addr = &pages[0];
    int *last_addr = &pages[npages * PAGE_SIZE / sizeof(int) - 1];

    char cmd[200];
    snprintf(cmd, 200, "sudo ./pagemap %d %p %p", getpid(), first_addr, last_addr);

    system(cmd);
}
