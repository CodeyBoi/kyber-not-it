#define __GNU_SOURCE

#define PAGE_SIZE 4096

#include <stdio.h>
#include <stdint.h>
#include <assert.h>
#include <inttypes.h>
#include <sys/mman.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdlib.h>
#include <math.h>
#include <sys/sysinfo.h>
#include <sys/personality.h>

#include <sched.h>
#include <inttypes.h>
#include <time.h>

#include "page.c"

int main(int argc, char *argv[])
{
    char cmd[200];
    cpu_set_t set;
    pid_t pid = getpid();

    int target_page = argv[1] ? atoi(argv[1]) : 5;
    double alloc_factor = argv[2] ? atof(argv[2]) : 1.0;
    int show_all_pages = argc > 3 && strcmp(argv[3], "-a") == 0;
    int do_func_call = argc > 4 && strcmp(argv[4], "-t") == 0;

    CPU_ZERO(&set);
    CPU_SET(1, &set);
    if (sched_setaffinity(pid, sizeof(set), &set) == -1)
        printf("ERROR WITH SCHEDAFFINITY");

    // snprintf(cmd, 200, "sudo taskset -p 0x2 %d", pid);
    // system(cmd);

    int *pages = mmap(NULL, target_page * PAGE_SIZE, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_POPULATE, -1, 0);
    if (pages == MAP_FAILED)
    {
        perror("mmap");
        exit(1);
    }

    snprintf(cmd, 200, "sudo ./pagemap %d %p %p", pid, &pages[0], &pages[target_page * PAGE_SIZE / sizeof(int) - 1]);
    system(cmd);
    snprintf(cmd, 200, "sudo taskset 0x2 ./pagetest %d %s", (int)(target_page * alloc_factor), show_all_pages ? "-a" : "");

    for (int i = 0; i < target_page; i++)
    {
        pages[i * PAGE_SIZE / sizeof(int)] = i + 12034;
    }
    for (int i = 0; i < target_page; i++)
    {
        munmap(&pages[i * PAGE_SIZE / sizeof(int)], PAGE_SIZE);
    }

    if (do_func_call)
        testpage((int)(target_page * alloc_factor), show_all_pages);
    else
        system(cmd);
}
