# Dockerfile for mcp-polyhedral
#
# Builds the full polyhedral_common C++ library (including POLY_dual_description)
# and then compiles the Rust MCP server on top of it.
#
# Build:
#   docker build -t mcp-polyhedral .
# Run:
#   docker run -i mcp-polyhedral

FROM ubuntu:24.04

LABEL maintainer="mathieu.dutour@gmail.com"

# ---------------------------------------------------------------------------
# Base system
# ---------------------------------------------------------------------------

RUN apt-get update -y
RUN DEBIAN_FRONTEND="noninteractive" apt-get -y install tzdata

RUN apt-get install -y libgdbm-dev libsnappy-dev make pkg-config rlwrap screen \
        software-properties-common sudo unzip zlib1g-dev zsh
RUN apt-get install -y wget curl emacs joe
RUN apt-get install -y g++ gcc gfortran clang
RUN apt-get install -y git cmake

# ---------------------------------------------------------------------------
# MPI
# ---------------------------------------------------------------------------

RUN apt-get install -y openmpi-bin libopenmpi-dev

# ---------------------------------------------------------------------------
# GMP
# ---------------------------------------------------------------------------

RUN apt-get install -y libgmp-dev
ENV GMP_INCDIR="/usr/include"
ENV GMP_CXX_LINK="-lgmp -lgmpxx"

# ---------------------------------------------------------------------------
# Boost
# ---------------------------------------------------------------------------

RUN apt-get install -y libboost-dev libboost-mpi-dev libboost-serialization-dev
ENV BOOST_INCDIR="/usr/include"
ENV BOOST_LINK="-lboost_serialization"
ENV MPI_LINK_CPP="-lboost_mpi"

# ---------------------------------------------------------------------------
# Eigen
# ---------------------------------------------------------------------------

RUN mkdir -p /opt
RUN cd /opt && git clone https://gitlab.com/libeigen/eigen.git
ENV EIGEN_PATH=/opt/eigen

# ---------------------------------------------------------------------------
# TBB
# ---------------------------------------------------------------------------

RUN apt-get install -y libtbb-dev
ENV TBB_INCDIR=/usr/include
ENV TBB_LINK="-ltbb"

# ---------------------------------------------------------------------------
# GLPK
# ---------------------------------------------------------------------------

RUN apt-get install -y libglpk-dev
ENV GLPK_PATH=/usr
ENV GLPK_INCLUDE="-I$GLPK_PATH/include"
ENV GLPK_LINK="-L$GLPK_PATH/lib -lglpk -Wl,-rpath,$GLPK_PATH/lib"

# ---------------------------------------------------------------------------
# Flint / NTL
# ---------------------------------------------------------------------------

RUN apt-get install -y libflint-dev libntl-dev
ENV FLINT_INCDIR=/include
ENV FLINT_LINK=-lflint

# ---------------------------------------------------------------------------
# cddlib
# ---------------------------------------------------------------------------

RUN apt-get install -y autoconf autoconf-archive autotools-dev libtool
RUN apt-get install -y texlive-latex-base
RUN git clone https://github.com/cddlib/cddlib
RUN cd cddlib && ./bootstrap && ./configure --prefix=/opt/cddlib && make && make install
ENV CDDLIB_PATH=/opt/cddlib
ENV CDDLIB_INCLUDE="-I$CDDLIB_PATH/include/cddlib"
ENV CDDLIB_DOUBLE_LINK="-L$CDDLIB_PATH/lib -lcdd -Wl,-rpath,$CDDLIB_PATH/lib"
ENV CDDLIB_GMP_LINK="-L$CDDLIB_PATH/lib -lcddgmp -Wl,-rpath,$CDDLIB_PATH/lib"

# ---------------------------------------------------------------------------
# PPL
# ---------------------------------------------------------------------------

RUN git clone https://github.com/BUGSENG/PPL
RUN cd PPL && autoreconf -i && ./configure && make
RUN cd /bin && ln -s /PPL/demos/ppl_lcdd/ppl_lcdd

# ---------------------------------------------------------------------------
# HDF5
# ---------------------------------------------------------------------------

RUN wget https://support.hdfgroup.org/ftp/HDF5/releases/hdf5-1.12/hdf5-1.12.0/src/hdf5-1.12.0.tar.gz
RUN tar -vxzf hdf5-1.12.0.tar.gz && cd hdf5-1.12.0 \
        && ./configure --with-default-api-version=v18 --prefix=/opt/hdf5-1.12.0 \
        && make && make install

# ---------------------------------------------------------------------------
# NetCDF
# ---------------------------------------------------------------------------

RUN apt-get install -y libcurl4-openssl-dev m4
RUN wget https://www.gfd-dennou.org/arch/netcdf/unidata-mirror/netcdf-c-4.8.0.tar.gz
RUN wget https://fossies.org/linux/misc/netcdf-cxx4-4.3.1.tar.gz
RUN tar -vxzf netcdf-c-4.8.0.tar.gz && cd netcdf-c-4.8.0 \
        && export LDFLAGS="-L/opt/hdf5-1.12.0/lib -lhdf5_hl -lhdf5 -Wl,-rpath,/opt/hdf5-1.12.0/lib" \
        && export CPPFLAGS="-I/opt/hdf5-1.12.0/include" \
        && ./configure --prefix=/opt/netcdf-4.8.0 && make && make install
RUN tar -vxzf netcdf-cxx4-4.3.1.tar.gz && cd netcdf-cxx4-4.3.1 \
        && export LDFLAGS="-L/opt/netcdf-4.8.0/lib -lnetcdf -Wl,-rpath,/opt/netcdf-4.8.0/lib" \
        && export CPPFLAGS="-I/opt/netcdf-4.8.0/include -I/opt/hdf5-1.12.0/include" \
        && ./configure --prefix=/opt/netcdf-cxx4-4.3.1 && make && make install
ENV NETCDF_CXX_PATH=/opt/netcdf-cxx4-4.3.1
ENV NETCDF_C_PATH=/opt/netcdf-4.8.0
ENV NETCDF_CXX_ALLINC="-I$NETCDF_CXX_PATH/include -I$NETCDF_C_PATH/include"
ENV NETCDF_CXX_LINK="-L$NETCDF_CXX_PATH/lib -lnetcdf_c++4 -Wl,-rpath,$NETCDF_CXX_PATH/lib"

# ---------------------------------------------------------------------------
# LinBox
# ---------------------------------------------------------------------------

RUN wget https://raw.githubusercontent.com/linbox-team/linbox/master/linbox-auto-install.sh
RUN chmod +x linbox-auto-install.sh
RUN ./linbox-auto-install.sh --enable-openblas=yes
RUN mkdir /opt/linbox \
        && mv /tmp/lib /opt/linbox \
        && mv /tmp/include /opt/linbox \
        && mv /tmp/bin /opt/linbox
ENV LINBOX_PATH=/opt/linbox
ENV LINBOX_INCLUDE="-DHAVE_CONFIG_H -DDISABLE_COMMENTATOR -DNDEBUG -UDEBUG -fopenmp -I$LINBOX_PATH/include"
ENV LINBOX_LINK="-fopenmp -lntl -lflint -L$LINBOX_PATH/lib -lopenblas -lpthread -lgfortran -lgivaro -lgmpxx -lgmp -Wl,-rpath,$CDDLIB_PATH/lib"

# ---------------------------------------------------------------------------
# polyhedral_common
# ---------------------------------------------------------------------------

RUN mkdir -p /GIT
RUN cd /GIT && git clone https://github.com/MathieuDutSik/polyhedral_common.git --recursive

# bliss
RUN cd /GIT && git clone https://github.com/MathieuDutSik/bliss
RUN cd /GIT/bliss && mkdir build && cd build && cmake .. && make
ENV LIBBLISS_INCDIR=/GIT/bliss/src
ENV LIBBLISS_LINK="-L/GIT/bliss/build -lbliss -Wl,-rpath,/GIT/bliss/build"

# nauty/traces
ENV NAUTY_PATH=/opt/nauty
ENV NAUTY_INCLUDE="-I$NAUTY_PATH/include"
ENV NAUTY_LINK="-L$NAUTY_PATH/lib -lnauty_shared -Wl,-rpath,$NAUTY_PATH/lib"
RUN cd /GIT/polyhedral_common/basic_common_cpp/ExternalLib/nauty && mkdir build
RUN cd /GIT/polyhedral_common/basic_common_cpp/ExternalLib/nauty/build \
        && cmake -DCMAKE_INSTALL_PREFIX:PATH=$NAUTY_PATH ..
RUN cd /GIT/polyhedral_common/basic_common_cpp/ExternalLib/nauty/build \
        && make all install

# Compile all polyhedral tools (produces POLY_dual_description in src_poly/)
RUN cd /GIT/polyhedral_common && ./compile.sh

# ---------------------------------------------------------------------------
# Rust toolchain
# ---------------------------------------------------------------------------

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"

# ---------------------------------------------------------------------------
# mcp-polyhedral (this repo)
# ---------------------------------------------------------------------------

WORKDIR /opt/mcp-polyhedral
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

# ---------------------------------------------------------------------------
# Runtime
# ---------------------------------------------------------------------------

# POLY_dual_description is at /GIT/polyhedral_common/src_poly/ (already in PATH
# via find_polyhedral_binary's hardcoded candidate list).
CMD ["/opt/mcp-polyhedral/target/release/mcp-polyhedral"]
