# CP/M Test Harness

This project contains a rust(rocket) web application which acts as a test harness for the various test ROMs produced against the CP/M operating system.

It implements the minimal BIOS functions (C_WRITE, C_WRITESTR) to execute those tests and outputs the results to the command line.

## Usage

This application can be used to validate the 8080 implementation using CP/M based test ROMs. It can be run with docker-compose by running

```
docker-compose up --build
```

To test a ROM you can then do

```
curl -X POST -F 'rom=@/my/file/rom.com' localhost:8080/api/v1/load
```

All logs are shipped to Loki (except that Loki can't keep up with the speed of log input so you'll only actually get some of them - gogogo distributed logging!). In most ROMs results are printed out using a `CALL 05` (CP/M bios WRITE commands) which we monkey patch to be `OUT 0`. The cpm-test-harness docker image will output any results to `stdout` where you can view the results either in the docker-compose output or the specific image logs.

Output can be viewed by looking at the container logs:

```
docker logs  cpm-test-harness_cpm-test-harness_1
```

which on a succesful run of `8080PRE.COM` will return

```
C_WRITESTR 818
8080 Preliminary tests complete
```