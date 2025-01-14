; Declare the string constant as a global constant.
@.str = private unnamed_addr constant [13 x i8] c"hello world\0A\00"

; External declaration of the puts function
declare i32 @"non trivial name"(ptr nocapture) nounwind

; Definition of main function
define i32 @main(i32 %0, ptr %1) {
  ; Call puts function to write out the string to stdout.
  call i32 @"non trivial name"(ptr @.str)
  ret i32 0
}

; Named metadata
!0 = !{i32 42, null, !"string"}
!foo = !{!0}
@glb = global i8 0

define void @f(ptr %a) {
  %c = icmp eq ptr %a, @glb
  br i1 %c, label %BB_EXIT, label %BB_CONTINUE ; escapes %a
BB_EXIT:
  call void @exit()
  unreachable
BB_CONTINUE:
  ret void
}