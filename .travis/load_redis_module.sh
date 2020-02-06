#!/bin/bash

if [ $TRAVIS_OS_NAME = linux ]
then
    sudo service redis-server stop
    lib_ext=so
else
    brew services stop redis
    lib_ext=dylib
fi

redis-server --loadmodule target/$TARGET/debug/libredis_shield.$lib_ext --daemonize yes
