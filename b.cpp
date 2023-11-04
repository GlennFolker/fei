#include <iostream>
#include "a.hpp"

template<> void my_stuff<int>::do_stuff() {
    std::cout << "Hello, world!" << std::endl;
}
