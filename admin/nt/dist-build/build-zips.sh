#!/bin/bash

## Copyright (C) 2017-2019 Free Software Foundation, Inc.

## This file is part of GNU Emacs.

## GNU Emacs is free software: you can redistribute it and/or modify
## it under the terms of the GNU General Public License as published by
## the Free Software Foundation, either version 3 of the License, or
## (at your option) any later version.

## GNU Emacs is distributed in the hope that it will be useful,
## but WITHOUT ANY WARRANTY; without even the implied warranty of
## MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
## GNU General Public License for more details.

## You should have received a copy of the GNU General Public License
## along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.


function git_up {
    echo Making git worktree for Emacs $VERSION
    cd $HOME/emacs-build/git/emacs-$MAJOR_VERSION
    git pull
    git worktree add ../$BRANCH $BRANCH

    cd ../$BRANCH
    ./autogen.sh
}

function build_zip {

    ARCH=$1
    PKG=$2
    HOST=$3

    echo Building Emacs-$VERSION for $ARCH
    if [ $ARCH == "i686" ]
    then
        PATH=/mingw32/bin:$PATH
        MSYSTEM=MINGW32
    fi

    ## Clean the install location because we use it twice
    rm -rf $HOME/emacs-build/install/emacs-$VERSION/$ARCH
    mkdir --parents $HOME/emacs-build/build/emacs-$VERSION/$ARCH
    cd $HOME/emacs-build/build/emacs-$VERSION/$ARCH

    export PKG_CONFIG_PATH=$PKG
    ../../../git/$BRANCH/configure \
        --without-dbus \
        --host=$HOST --without-compress-install \
        $CACHE \
        CFLAGS="-O2 -static -g3"
    make -j 16 install \
         prefix=$HOME/emacs-build/install/emacs-$VERSION/$ARCH
    cd $HOME/emacs-build/install/emacs-$VERSION/$ARCH
    cp $HOME/emacs-build/deps/libXpm/$ARCH/libXpm-noX4.dll bin
    zip -r -9 emacs-$VERSION-$ARCH-no-deps.zip *
    mv emacs-$VERSION-$ARCH-no-deps.zip $HOME/emacs-upload
    rm bin/libXpm-noX4.dll
    unzip $HOME/emacs-build/deps/emacs-26-$ARCH-deps.zip
    zip -r -9 emacs-$VERSION-$ARCH.zip *
    mv emacs-$VERSION-$ARCH.zip ~/emacs-upload
}

function build_installer {
    ARCH=$1
    cd $HOME/emacs-build/install/emacs-$VERSION
    echo Calling makensis in `pwd`
    cp ../../git/$BRANCH/admin/nt/dist-build/emacs.nsi .
    makensis -DARCH=$ARCH -DEMACS_VERSION=$ACTUAL_VERSION \
             -DOUT_VERSION=$VERSION emacs.nsi
    rm emacs.nsi
    mv Emacs-$ARCH-$VERSION-installer.exe ~/emacs-upload
}

set -o errexit

SNAPSHOT=
CACHE=

BUILD=1
BUILD_32=1
BUILD_64=1
GIT_UP=0

while getopts "36ghsiV:" opt; do
  case $opt in
    3)
        BUILD_32=1
        BUILD_64=0
        GIT_UP=0
        ;;
    6)
        BUILD_32=0
        BUILD_64=1
        GIT_UP=0
        ;;

    g)
        BUILD_32=0
        BUILD_64=0
        GIT_UP=1
        ;;
    i)
        BUILD=1
        ;;
    V)
        VERSION=$OPTARG
        ;;
    s)
        SNAPSHOT="-snapshot"
        ;;
    h)
        echo "build-zips.sh"
        echo "  -3 32 bit build only"
        echo "  -6 64 bit build only"
        echo "  -g git update and worktree only"
        exit 0
        ;;
    \?)
        echo "Invalid option: -$OPTARG" >&2
        ;;
  esac
done

if [ -z $VERSION ];
then
    VERSION=`
  sed -n 's/^AC_INIT(GNU Emacs,[	 ]*\([^	 ,)]*\).*/\1/p' < ../../../configure.ac
`
fi

if [ -z $VERSION ];
then
    echo Cannot determine Emacs version
    exit 1
fi

MAJOR_VERSION="$(echo $VERSION | cut -d'.' -f1)"

if [ -z $SNAPSHOT ];
then
    BRANCH=emacs-$VERSION
else
    BRANCH=master
    CACHE=-C
fi

ACTUAL_VERSION=$VERSION
VERSION=$VERSION$SNAPSHOT

if (($GIT_UP))
then
    git_up
fi

if (($BUILD_64))
then
    if (($BUILD))
    then
        build_zip x86_64 /mingw64/lib/pkgconfig x86_64-w64-mingw32
    fi
    build_installer x86_64
fi

## Do the 64 bit build first, because we reset some environment
## variables during the 32 bit which will break the build.
if (($BUILD_32))
then
    if (($BUILD))
    then
        build_zip i686 /mingw32/lib/pkgconfig i686-w64-mingw32
    fi
    build_installer i686
fi
