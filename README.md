# Why?
I made this to see how closely I could emulate the working of (p)locate on linux, and if I could make it work on windows

# Roadmap
## Although the current version is completely functional, This is still very much in development.

### 1. Implement a better way to store the file paths
maybe just empty directories and files and skip the directories, instead of storing each and every path, works but is inefficient

### 2. Come up with a better way to update the database
just updates it idiomatically, dropping the table and creating a new one from scratch, maybe check if all the files in the database exist and delete the ones that don't and add new files in parallel? might change the approach if this turns out to be innefficient

### 3. Add a check to ensure only the files a user can access shows up on the database
if you create a database with sudo(or run as admin on windows) you end up being able to search for files you can't access which is, well problematic

### 4. Fix the bugs I inevitably introduce in steps 1-3